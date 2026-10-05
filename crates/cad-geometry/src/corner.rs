//! Corners between two lines (DESIGN §22): the common ground of FILLET and
//! CHAMFER.
//!
//! Two lines, carried on if need be, meet at an *apex*. Each line has a side
//! that is kept — the side it was picked on. Both operations cut each line
//! back, or carry it on, to a point a certain *setback* from the apex along
//! its kept side, and then join those two points: with a tangent arc, or
//! with a straight bevel.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};

use crate::intersect::line_line;
use crate::{Arc, Segment, normalize_angle};

/// Why two lines could not be joined at a corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerError {
    /// The lines are parallel: they have no apex.
    Parallel,
    /// A line is too short for the radius or distance asked for.
    TooLarge,
    /// No arc of that radius is tangent to both objects — or, for a radius
    /// of zero, the objects never meet.
    NoSolution,
}

/// One line of a corner, seen from the apex.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Leg {
    /// Unit direction from the apex along the kept side.
    direction: Vec2,
    /// The end of the line on the kept side: it stays where it is.
    far: Point2,
    /// Distance from the apex to `far`.
    reach: f64,
    /// Whether `far` is the segment's end (rather than its start), so the
    /// result keeps the line's direction.
    far_is_end: bool,
}

impl Leg {
    fn new(line: &Segment, pick: Point2, apex: Point2, tolerance: LinearTolerance) -> Self {
        // The kept side is the side of the apex the line was picked on. A
        // pick at the apex itself says nothing: keep the longer side.
        let picked = line.closest_point(pick);
        let towards = if tolerance.coincident(picked, apex) {
            let (to_start, to_end) = (line.start - apex, line.end - apex);
            if to_end.length() >= to_start.length() {
                to_end
            } else {
                to_start
            }
        } else {
            picked - apex
        };
        let direction = towards * (1.0 / towards.length());
        let along = |point: Point2| (point - apex).dot(direction);
        let far_is_end = along(line.end) >= along(line.start);
        let far = if far_is_end { line.end } else { line.start };
        Self {
            direction,
            far,
            reach: along(far),
            far_is_end,
        }
    }

    /// The line from `setback` along the kept side to its far end.
    fn cut_back(
        &self,
        apex: Point2,
        setback: f64,
        tolerance: LinearTolerance,
    ) -> Result<(Segment, Point2), CornerError> {
        if setback >= self.reach || tolerance.eq(setback, self.reach) {
            return Err(CornerError::TooLarge);
        }
        let joint = apex + self.direction * setback;
        let line = if self.far_is_end {
            Segment::new(joint, self.far)
        } else {
            Segment::new(self.far, joint)
        };
        Ok((line, joint))
    }
}

/// Two lines and the point where they meet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    apex: Point2,
    legs: [Leg; 2],
}

/// Two lines joined by a tangent arc; no arc when the radius is zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fillet {
    pub lines: [Segment; 2],
    pub arc: Option<Arc>,
}

/// Two lines joined by a straight bevel; none when both distances are zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chamfer {
    pub lines: [Segment; 2],
    pub bevel: Option<Segment>,
}

impl Corner {
    /// The corner of two lines, each picked on the side to keep.
    pub fn new(
        first: &Segment,
        first_pick: Point2,
        second: &Segment,
        second_pick: Point2,
        tolerance: LinearTolerance,
    ) -> Result<Self, CornerError> {
        let degenerate = first.is_degenerate(tolerance) || second.is_degenerate(tolerance);
        let apex = line_line(first, second)
            .filter(|apex| apex.is_finite() && !degenerate)
            .ok_or(CornerError::Parallel)?;
        Ok(Self {
            apex,
            legs: [
                Leg::new(first, first_pick, apex, tolerance),
                Leg::new(second, second_pick, apex, tolerance),
            ],
        })
    }

    pub fn apex(&self) -> Point2 {
        self.apex
    }

    /// Both lines cut back to their setbacks, and the two joints.
    fn cut_back(
        &self,
        setbacks: [f64; 2],
        tolerance: LinearTolerance,
    ) -> Result<([Segment; 2], [Point2; 2]), CornerError> {
        let (first, first_joint) = self.legs[0].cut_back(self.apex, setbacks[0], tolerance)?;
        let (second, second_joint) = self.legs[1].cut_back(self.apex, setbacks[1], tolerance)?;
        Ok(([first, second], [first_joint, second_joint]))
    }

    /// The lines joined by an arc of `radius` tangent to both. A radius of
    /// zero makes a sharp corner at the apex.
    pub fn fillet(&self, radius: f64, tolerance: LinearTolerance) -> Result<Fillet, CornerError> {
        if tolerance.is_zero(radius) {
            let (lines, _) = self.cut_back([0.0, 0.0], tolerance)?;
            return Ok(Fillet { lines, arc: None });
        }
        let [a, b] = [self.legs[0].direction, self.legs[1].direction];
        // θ is the angle between the kept sides: 0 < θ < π for lines that meet.
        let cosine = a.dot(b).clamp(-1.0, 1.0);
        let sine = (a.x * b.y - a.y * b.x).abs();
        // 1 / tan(θ/2) and sin(θ/2), each written so that it never
        // subtracts two nearly equal numbers. `1 - cos θ` keeps only half
        // its digits by θ = 10⁻⁵ — lines a hair from parallel — and
        // `1 + cos θ` does the same near a straight angle; an arc placed
        // with either would miss the lines it is meant to touch.
        let (cotangent_half, sine_half) = if cosine > 0.0 {
            ((1.0 + cosine) / sine, sine / (2.0 * (1.0 + cosine)).sqrt())
        } else {
            (sine / (1.0 - cosine), ((1.0 - cosine) / 2.0).sqrt())
        };
        // r / tan(θ/2) back along each line from the apex.
        let setback = radius * cotangent_half;
        let (lines, joints) = self.cut_back([setback, setback], tolerance)?;

        let bisector = a + b;
        // r / sin(θ/2) along the bisector to the centre.
        let to_center = radius / sine_half;
        let center = self.apex + bisector * (to_center / bisector.length());
        let angle = |point: Point2| {
            let offset = point - center;
            offset.y.atan2(offset.x)
        };
        // The tangent arc is the short way round between the two joints.
        let (from, to) = (angle(joints[0]), angle(joints[1]));
        let arc = if normalize_angle(to - from) < std::f64::consts::PI {
            Arc::from_angles(center, radius, from, to)
        } else {
            Arc::from_angles(center, radius, to, from)
        };
        Ok(Fillet {
            lines,
            arc: Some(arc),
        })
    }

    /// The lines cut back by `distances` from the apex — the first distance
    /// along the first line — and joined by a straight bevel.
    pub fn chamfer(
        &self,
        distances: [f64; 2],
        tolerance: LinearTolerance,
    ) -> Result<Chamfer, CornerError> {
        let (lines, joints) = self.cut_back(distances, tolerance)?;
        let bevel = (!tolerance.coincident(joints[0], joints[1]))
            .then(|| Segment::new(joints[0], joints[1]));
        Ok(Chamfer { lines, bevel })
    }
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
        a.distance_to(b) < 1e-9
    }

    /// An L: a horizontal line along +X and a vertical one along +Y, which
    /// stop short of the origin where they would meet.
    fn horizontal() -> Segment {
        Segment::new(p(10.0, 0.0), p(100.0, 0.0))
    }

    fn vertical() -> Segment {
        Segment::new(p(0.0, 80.0), p(0.0, 10.0))
    }

    fn l_corner() -> Corner {
        Corner::new(
            &horizontal(),
            p(50.0, 0.0),
            &vertical(),
            p(0.0, 50.0),
            TOLERANCE,
        )
        .unwrap()
    }

    #[test]
    fn the_apex_is_where_the_lines_would_meet() {
        assert_eq!(l_corner().apex(), p(0.0, 0.0));
    }

    #[test]
    fn a_zero_radius_carries_both_lines_on_to_a_sharp_corner() {
        let fillet = l_corner().fillet(0.0, TOLERANCE).unwrap();
        assert_eq!(fillet.arc, None);
        // Each line keeps its direction: the horizontal still runs left to right.
        assert_eq!(fillet.lines[0], Segment::new(p(0.0, 0.0), p(100.0, 0.0)));
        assert_eq!(fillet.lines[1], Segment::new(p(0.0, 80.0), p(0.0, 0.0)));
    }

    #[test]
    fn a_fillet_of_a_right_angle_is_a_quarter_arc_tangent_to_both_lines() {
        let fillet = l_corner().fillet(20.0, TOLERANCE).unwrap();
        assert_eq!(fillet.lines[0], Segment::new(p(20.0, 0.0), p(100.0, 0.0)));
        assert_eq!(fillet.lines[1], Segment::new(p(0.0, 80.0), p(0.0, 20.0)));

        let arc = fillet.arc.unwrap();
        assert!(close(arc.center, p(20.0, 20.0)));
        assert!((arc.radius - 20.0).abs() < 1e-12);
        assert!((arc.sweep - FRAC_PI_2).abs() < 1e-12, "the short way round");
        // It runs between the two joints…
        let ends = [arc.start_point(), arc.end_point()];
        assert!(ends.iter().any(|end| close(*end, p(20.0, 0.0))));
        assert!(ends.iter().any(|end| close(*end, p(0.0, 20.0))));
        // …and bulges towards the apex.
        assert!(arc.mid_point().distance_to(p(0.0, 0.0)) < 20.0);
    }

    #[test]
    fn a_fillet_trims_lines_that_cross_keeping_the_picked_sides() {
        // An X through the origin.
        let rising = Segment::new(p(-50.0, -50.0), p(50.0, 50.0));
        let falling = Segment::new(p(-50.0, 50.0), p(50.0, -50.0));
        // Keep the upper-right of one and the lower-right of the other.
        let corner =
            Corner::new(&rising, p(30.0, 30.0), &falling, p(30.0, -30.0), TOLERANCE).unwrap();
        let fillet = corner.fillet(10.0, TOLERANCE).unwrap();
        assert!(close(fillet.lines[0].end, p(50.0, 50.0)));
        assert!(close(fillet.lines[1].end, p(50.0, -50.0)));
        // A right angle again: setback 10, joints at 10/√2 on each axis.
        let joint = 10.0 / 2f64.sqrt();
        assert!(close(fillet.lines[0].start, p(joint, joint)));
        assert!(close(fillet.lines[1].start, p(joint, -joint)));
        assert!(close(
            fillet.arc.unwrap().center,
            p(10.0 * 2f64.sqrt(), 0.0)
        ));
    }

    /// Regression: found by the stress tests. With the half angle taken from
    /// `1 - cos θ`, the arc between lines 10⁻⁵ rad apart sat 3.4 × 10⁻⁶ off
    /// tangent; the same happened near a straight angle with `1 + cos θ`.
    #[test]
    fn a_fillet_between_lines_a_hair_from_parallel_is_still_tangent() {
        for angle in [1e-5_f64, 1e-7, std::f64::consts::PI - 1e-5] {
            let first = Segment::new(p(0.0, 0.0), p(100.0, 0.0));
            let second = Segment::new(p(0.0, 0.0), p(100.0 * angle.cos(), 100.0 * angle.sin()));
            let corner = Corner::new(&first, first.end, &second, second.end, TOLERANCE).unwrap();
            // The radius that sets each line back by 20 from the corner.
            let radius = 20.0 * (angle / 2.0).tan();
            let fillet = corner.fillet(radius, TOLERANCE).unwrap();
            let arc = fillet.arc.expect("a radius was given");
            for line in [&first, &second] {
                let away = line_distance(line, arc.center);
                assert!(
                    (away - radius).abs() < radius * 1e-9,
                    "at {angle}: {away} from a line, radius {radius}"
                );
            }
        }
    }

    #[test]
    fn parallel_lines_have_no_corner() {
        let other = Segment::new(p(0.0, 30.0), p(100.0, 30.0));
        assert_eq!(
            Corner::new(
                &horizontal(),
                p(50.0, 0.0),
                &other,
                p(50.0, 30.0),
                TOLERANCE
            ),
            Err(CornerError::Parallel)
        );
        let collinear = Segment::new(p(200.0, 0.0), p(300.0, 0.0));
        assert_eq!(
            Corner::new(
                &horizontal(),
                p(50.0, 0.0),
                &collinear,
                p(250.0, 0.0),
                TOLERANCE
            ),
            Err(CornerError::Parallel)
        );
        let dot = Segment::new(p(5.0, 5.0), p(5.0, 5.0));
        assert_eq!(
            Corner::new(&horizontal(), p(50.0, 0.0), &dot, p(5.0, 5.0), TOLERANCE),
            Err(CornerError::Parallel)
        );
    }

    #[test]
    fn a_radius_that_swallows_a_line_is_too_large() {
        // The vertical line reaches 80 from the apex; the setback equals the radius here.
        assert!(l_corner().fillet(79.0, TOLERANCE).is_ok());
        assert_eq!(
            l_corner().fillet(80.0, TOLERANCE),
            Err(CornerError::TooLarge)
        );
        assert_eq!(
            l_corner().fillet(500.0, TOLERANCE),
            Err(CornerError::TooLarge)
        );
    }

    #[test]
    fn a_chamfer_cuts_each_line_back_by_its_own_distance() {
        let chamfer = l_corner().chamfer([30.0, 10.0], TOLERANCE).unwrap();
        assert_eq!(chamfer.lines[0], Segment::new(p(30.0, 0.0), p(100.0, 0.0)));
        assert_eq!(chamfer.lines[1], Segment::new(p(0.0, 80.0), p(0.0, 10.0)));
        assert_eq!(
            chamfer.bevel,
            Some(Segment::new(p(30.0, 0.0), p(0.0, 10.0)))
        );

        let equal = l_corner().chamfer([15.0, 15.0], TOLERANCE).unwrap();
        assert_eq!(equal.bevel, Some(Segment::new(p(15.0, 0.0), p(0.0, 15.0))));
    }

    #[test]
    fn a_chamfer_of_zero_distances_is_a_sharp_corner() {
        let chamfer = l_corner().chamfer([0.0, 0.0], TOLERANCE).unwrap();
        assert_eq!(chamfer.bevel, None);
        assert_eq!(chamfer.lines[0], Segment::new(p(0.0, 0.0), p(100.0, 0.0)));
        // One zero distance still makes a bevel, from the apex.
        let one = l_corner().chamfer([0.0, 25.0], TOLERANCE).unwrap();
        assert_eq!(one.bevel, Some(Segment::new(p(0.0, 0.0), p(0.0, 25.0))));
    }

    #[test]
    fn chamfer_distances_that_swallow_a_line_are_too_large() {
        assert_eq!(
            l_corner().chamfer([100.0, 10.0], TOLERANCE),
            Err(CornerError::TooLarge)
        );
        assert_eq!(
            l_corner().chamfer([10.0, 80.0], TOLERANCE),
            Err(CornerError::TooLarge)
        );
        assert!(l_corner().chamfer([99.0, 79.0], TOLERANCE).is_ok());
    }

    #[test]
    fn a_pick_at_the_apex_keeps_the_longer_side() {
        let through = Segment::new(p(-20.0, 0.0), p(100.0, 0.0));
        let corner =
            Corner::new(&through, p(0.0, 0.0), &vertical(), p(0.0, 50.0), TOLERANCE).unwrap();
        let fillet = corner.fillet(0.0, TOLERANCE).unwrap();
        assert_eq!(fillet.lines[0], Segment::new(p(0.0, 0.0), p(100.0, 0.0)));
    }

    proptest! {
        /// #99: the arc is tangent to both lines — at each joint, the radius
        /// is perpendicular to the line, and the joint is on the line.
        #[test]
        fn a_fillet_arc_is_tangent_to_both_lines(
            degrees in 15.0..165f64, turn in 0.0..360f64, radius in 1.0..30f64,
            ax in -100.0..100f64, ay in -100.0..100f64,
        ) {
            let apex = p(ax, ay);
            let a = crate::unit_at_degrees(turn);
            let b = crate::unit_at_degrees(turn + degrees);
            let first = Segment::new(apex + a * 5.0, apex + a * 400.0);
            let second = Segment::new(apex + b * 400.0, apex + b * 5.0);
            let corner = Corner::new(&first, apex + a * 200.0, &second, apex + b * 200.0, TOLERANCE).unwrap();
            let fillet = corner.fillet(radius, TOLERANCE).unwrap();
            let arc = fillet.arc.unwrap();

            for (line, along) in [(first, a), (second, b)] {
                let joint = [arc.start_point(), arc.end_point()]
                    .into_iter()
                    .min_by(|x, y| line_distance(&line, *x).total_cmp(&line_distance(&line, *y)))
                    .unwrap();
                prop_assert!(line_distance(&line, joint) < 1e-6, "the joint is off the line");
                let radius_direction = joint - arc.center;
                prop_assert!(radius_direction.dot(along).abs() < 1e-6 * radius, "not tangent");
            }
            prop_assert!((arc.radius - radius).abs() < 1e-9);
            prop_assert!(arc.sweep < std::f64::consts::PI);
            // The trimmed lines end on the arc.
            for line in fillet.lines {
                let nearer = arc.distance_to_point(line.start).min(arc.distance_to_point(line.end));
                prop_assert!(nearer < 1e-6);
            }
        }
    }

    /// Distance from a point to the infinite line through a segment.
    fn line_distance(line: &Segment, point: Point2) -> f64 {
        let along = line.end - line.start;
        let offset = point - line.start;
        (along.x * offset.y - along.y * offset.x).abs() / along.length()
    }
}
