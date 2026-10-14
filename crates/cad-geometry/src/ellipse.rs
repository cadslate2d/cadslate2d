//! Ellipses (#181): an ellipse, or an arc of one, by its centre, its major
//! half-axis, the minor axis as a ratio of it, and a parameter range.
//!
//! The curve is `centre + major·cos t + minor·sin t`, counter-clockwise in
//! `t`, with `minor` the major axis turned a quarter turn and scaled by the
//! ratio. The parameter is not the angle of the point from the centre, as
//! DXF's is not either; [`Ellipse::parameter_of`] turns one into the other.
//! Everything that needs the curve as lines — snapping, intersection,
//! rendering — takes its chords, [`CHORDS_PER_TURN`] to a full turn, whose
//! sagitta is a third of a thousandth of the major axis.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use cad_core::{Aabb, Point2, Vec2};

use crate::arc::normalize_angle;
use crate::segment::Segment;
use crate::transform::Transform;

/// Chords a full ellipse is read as.
pub const CHORDS_PER_TURN: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ellipse {
    pub center: Point2,
    /// The major half-axis: from the centre to one end of the long axis.
    pub major: Vec2,
    /// The minor half-axis's length over the major's, in `(0, 1]`.
    pub ratio: f64,
    /// Start parameter in `[0, 2π)`.
    pub start: f64,
    /// Parameter covered, in `(0, 2π]`; `2π` for a whole ellipse.
    pub sweep: f64,
}

impl Ellipse {
    /// A whole ellipse.
    pub fn full(center: Point2, major: Vec2, ratio: f64) -> Self {
        Self {
            center,
            major,
            ratio,
            start: 0.0,
            sweep: TAU,
        }
    }

    /// An arc of an ellipse, counter-clockwise from `start` to `end` in
    /// parameter; equal parameters give the whole ellipse.
    pub fn from_parameters(center: Point2, major: Vec2, ratio: f64, start: f64, end: f64) -> Self {
        let sweep = normalize_angle(end - start);
        Self {
            center,
            major,
            ratio,
            start: normalize_angle(start),
            sweep: if sweep > 0.0 { sweep } else { TAU },
        }
    }

    /// Whether the geometry is one: finite, a major axis with length, a
    /// ratio in `(0, 1]`.
    pub fn is_valid(&self) -> bool {
        self.center.is_finite()
            && self.major.x.is_finite()
            && self.major.y.is_finite()
            && self.major.length() > 0.0
            && self.ratio.is_finite()
            && self.ratio > 0.0
            && self.ratio <= 1.0
            && self.start.is_finite()
            && self.sweep.is_finite()
            && self.sweep > 0.0
    }

    /// The area of the whole ellipse: π times the two half-axes.
    pub fn area(&self) -> f64 {
        let major = self.major.length();
        std::f64::consts::PI * major * major * self.ratio
    }

    pub fn is_full(&self) -> bool {
        self.sweep >= TAU
    }

    /// The minor half-axis: the major turned a quarter turn, scaled.
    pub fn minor(&self) -> Vec2 {
        Vec2::new(-self.major.y, self.major.x) * self.ratio
    }

    pub fn end(&self) -> f64 {
        normalize_angle(self.start + self.sweep)
    }

    pub fn point_at(&self, parameter: f64) -> Point2 {
        let (sin, cos) = parameter.sin_cos();
        self.center + self.major * cos + self.minor() * sin
    }

    pub fn start_point(&self) -> Point2 {
        self.point_at(self.start)
    }

    pub fn end_point(&self) -> Point2 {
        self.point_at(self.start + self.sweep)
    }

    /// The parameter of the point of the whole ellipse in the direction of
    /// `point` from the centre, in `[0, 2π)`: what a pick on the curve
    /// means as a parameter.
    pub fn parameter_of(&self, point: Point2) -> f64 {
        let offset = point - self.center;
        let (a, b) = (self.major.length(), self.major.length() * self.ratio);
        let along = offset.dot(self.major) / a;
        let across = offset.dot(self.minor()) / b;
        normalize_angle((across / b).atan2(along / a))
    }

    /// Whether `parameter` lies on the arc.
    pub fn contains_parameter(&self, parameter: f64) -> bool {
        self.is_full() || normalize_angle(parameter - self.start) <= self.sweep
    }

    /// The points of the chords along the arc, from its start to its end.
    pub fn chord_points(&self) -> Vec<Point2> {
        let count = ((self.sweep / TAU) * CHORDS_PER_TURN as f64)
            .ceil()
            .max(8.0) as usize;
        (0..=count)
            .map(|i| self.point_at(self.start + self.sweep * i as f64 / count as f64))
            .collect()
    }

    /// The arc as chords.
    pub fn segments(&self) -> Vec<Segment> {
        self.chord_points()
            .windows(2)
            .map(|pair| Segment::new(pair[0], pair[1]))
            .collect()
    }

    /// The parameters of the arc's ends and of the extremes of the whole
    /// ellipse in x and y that the arc passes.
    fn extreme_parameters(&self) -> Vec<f64> {
        let (major, minor) = (self.major, self.minor());
        // d/dt of x and of y vanish where tan t = minor/major per axis.
        let mut parameters = vec![self.start, self.start + self.sweep];
        for base in [minor.x.atan2(major.x), minor.y.atan2(major.y)] {
            for turn in [base, base + PI] {
                if self.contains_parameter(turn) {
                    parameters.push(turn);
                }
            }
        }
        parameters
    }

    /// The axis-aligned box the arc fits in, exactly.
    pub fn bounds(&self) -> Aabb {
        let mut points = self
            .extreme_parameters()
            .into_iter()
            .map(|t| self.point_at(t));
        let first = points.next().expect("the start is always there");
        points.fold(Aabb::from_points(first, first), |bounds, point| {
            bounds.union(&Aabb::from_points(point, point))
        })
    }

    /// The parameters of the whole ellipse's axis ends the arc passes: its
    /// quadrant points.
    pub fn quadrant_parameters(&self) -> Vec<f64> {
        [0.0, FRAC_PI_2, PI, 3.0 * FRAC_PI_2]
            .into_iter()
            .filter(|t| self.contains_parameter(*t))
            .collect()
    }

    /// The point of the arc nearest to `point`: the nearest chord point
    /// finds the right stretch of the curve — a flat ellipse has two
    /// nearest candidates for a point near its centre — and Newton's steps
    /// on the parameter refine it; then the arc's ends.
    pub fn closest_point(&self, point: Point2) -> Point2 {
        let count = ((self.sweep / TAU) * CHORDS_PER_TURN as f64)
            .ceil()
            .max(8.0) as usize;
        let mut t = (0..=count)
            .map(|i| self.start + self.sweep * i as f64 / count as f64)
            .min_by(|a, b| {
                self.point_at(*a)
                    .distance_to(point)
                    .total_cmp(&self.point_at(*b).distance_to(point))
            })
            .expect("at least nine samples");
        let (major, minor) = (self.major, self.minor());
        for _ in 0..12 {
            let (sin, cos) = t.sin_cos();
            let p = self.center + major * cos + minor * sin;
            let d1 = minor * cos - major * sin;
            let d2 = major * (-cos) - minor * sin;
            let f = (p - point).dot(d1);
            let df = d1.dot(d1) + (p - point).dot(d2);
            if df.abs() < 1e-300 {
                break;
            }
            let next = t - f / df;
            if !next.is_finite() {
                break;
            }
            if (next - t).abs() < 1e-14 {
                t = next;
                break;
            }
            t = next;
        }
        let mut best = if self.contains_parameter(t) {
            Some(self.point_at(t))
        } else {
            None
        };
        for candidate in [self.start_point(), self.end_point()] {
            if best.is_none_or(|b| candidate.distance_to(point) < b.distance_to(point)) {
                best = Some(candidate);
            }
        }
        best.expect("the ends are always candidates")
    }

    pub fn distance_to_point(&self, point: Point2) -> f64 {
        self.closest_point(point).distance_to(point)
    }

    /// The image under a similarity: the axes turn and scale with it, and
    /// a mirror, which reverses the direction of travel, has the image run
    /// from the original's end.
    pub fn transformed(&self, transform: &Transform) -> Self {
        let center = transform.apply(self.center);
        let major = transform.apply(self.center + self.major) - center;
        if self.is_full() {
            return Self::full(center, major, self.ratio);
        }
        if transform.reverses_orientation() {
            // point(t) maps to the mirrored ellipse's point(−t).
            Self::from_parameters(
                center,
                major,
                self.ratio,
                -(self.start + self.sweep),
                -self.start,
            )
        } else {
            Self {
                center,
                major,
                ..*self
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn any_ellipse() -> impl Strategy<Value = Ellipse> {
        (
            -100.0f64..100.0,
            -100.0f64..100.0,
            1.0f64..50.0,
            0.0f64..TAU,
            0.05f64..=1.0,
            0.0f64..TAU,
            0.1f64..TAU,
        )
            .prop_map(|(cx, cy, a, angle, ratio, start, sweep)| Ellipse {
                center: p(cx, cy),
                major: Vec2::from_polar(a, angle),
                ratio,
                start: normalize_angle(start),
                sweep,
            })
    }

    #[test]
    fn a_round_ellipse_is_its_circle() {
        let ellipse = Ellipse::full(p(10.0, 5.0), Vec2::new(3.0, 0.0), 1.0);
        for t in [0.0, 1.0, 2.5, 4.0] {
            assert!((ellipse.point_at(t).distance_to(p(10.0, 5.0)) - 3.0).abs() < 1e-12);
        }
        assert!((ellipse.distance_to_point(p(17.0, 5.0)) - 4.0).abs() < 1e-9);
        assert!((ellipse.distance_to_point(p(10.0, 5.0)) - 3.0).abs() < 1e-9);
        assert_eq!(
            ellipse.bounds(),
            Aabb::from_points(p(7.0, 2.0), p(13.0, 8.0))
        );
    }

    #[test]
    fn the_axis_ends_and_bounds_follow_the_axes() {
        let ellipse = Ellipse::full(p(0.0, 0.0), Vec2::new(10.0, 0.0), 0.5);
        assert_eq!(ellipse.point_at(0.0), p(10.0, 0.0));
        assert!((ellipse.point_at(FRAC_PI_2).distance_to(p(0.0, 5.0))) < 1e-12);
        assert_eq!(
            ellipse.bounds(),
            Aabb::from_points(p(-10.0, -5.0), p(10.0, 5.0))
        );
        // Turned 90°: the long axis is vertical.
        let turned = Ellipse::full(p(0.0, 0.0), Vec2::new(0.0, 10.0), 0.5);
        let bounds = turned.bounds();
        assert!(bounds.min().distance_to(p(-5.0, -10.0)) < 1e-12);
        assert!(bounds.max().distance_to(p(5.0, 10.0)) < 1e-12);
        // A quarter arc from the major end to the minor end.
        let quarter =
            Ellipse::from_parameters(p(0.0, 0.0), Vec2::new(10.0, 0.0), 0.5, 0.0, FRAC_PI_2);
        let bounds = quarter.bounds();
        assert!(bounds.min().distance_to(p(0.0, 0.0)) < 1e-12);
        assert!(bounds.max().distance_to(p(10.0, 5.0)) < 1e-12);
        assert_eq!(quarter.quadrant_parameters().len(), 2);
    }

    #[test]
    fn the_parameter_is_not_the_angle() {
        let ellipse = Ellipse::full(p(0.0, 0.0), Vec2::new(10.0, 0.0), 0.5);
        // At 45° of parameter the point is (7.07, 3.54): 26.6° from the centre.
        let point = ellipse.point_at(FRAC_PI_2 / 2.0);
        assert!((point.y.atan2(point.x).to_degrees() - 26.565).abs() < 0.01);
        assert!((ellipse.parameter_of(point) - FRAC_PI_2 / 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_mirror_runs_the_arc_the_other_way_over_the_same_points() {
        let arc = Ellipse::from_parameters(p(0.0, 0.0), Vec2::new(10.0, 0.0), 0.5, 0.0, FRAC_PI_2);
        let mirror = Transform::mirror(p(0.0, -10.0), p(0.0, 10.0)).unwrap();
        let image = arc.transformed(&mirror);
        assert!(image.start_point().distance_to(p(0.0, 5.0)) < 1e-12);
        assert!(image.end_point().distance_to(p(-10.0, 0.0)) < 1e-12);
        assert!((image.sweep - FRAC_PI_2).abs() < 1e-12);
    }

    proptest! {
        #[test]
        fn every_point_of_the_arc_is_inside_its_bounds(ellipse in any_ellipse(), f in 0.0f64..=1.0) {
            let point = ellipse.point_at(ellipse.start + ellipse.sweep * f);
            let bounds = ellipse.bounds().expanded(1e-9 * (1.0 + ellipse.major.length()));
            prop_assert!(bounds.contains_point(point), "{point:?} outside {bounds:?}");
        }

        #[test]
        fn chords_hug_the_curve(ellipse in any_ellipse()) {
            let a = ellipse.major.length();
            for segment in ellipse.segments() {
                let mid = segment.start + (segment.end - segment.start) * 0.5;
                // The sagitta of a chord of 2π/128 on a circle of radius a.
                let sagitta = a * (1.0 - (PI / CHORDS_PER_TURN as f64).cos());
                prop_assert!(ellipse.distance_to_point(mid) <= sagitta * 1.01 + 1e-9);
            }
        }

        #[test]
        fn the_parameter_of_a_point_of_the_curve_is_its_parameter(ellipse in any_ellipse(), t in 0.0f64..TAU) {
            let found = ellipse.parameter_of(ellipse.point_at(t));
            let difference = normalize_angle(found - t);
            prop_assert!(difference < 1e-9 || difference > TAU - 1e-9, "{found} vs {t}");
        }

        #[test]
        fn the_closest_point_is_on_the_arc_and_no_chord_point_is_closer(ellipse in any_ellipse(), x in -200.0f64..200.0, y in -200.0f64..200.0) {
            let point = p(x, y);
            let closest = ellipse.closest_point(point);
            let t = ellipse.parameter_of(closest);
            prop_assert!(ellipse.contains_parameter(t) || closest.distance_to(ellipse.start_point()) < 1e-6 || closest.distance_to(ellipse.end_point()) < 1e-6);
            prop_assert!(closest.distance_to(ellipse.point_at(t)) < 1e-6 || ellipse.point_at(t).distance_to(point) >= closest.distance_to(point) - 1e-6);
            let distance = closest.distance_to(point);
            for chord_point in ellipse.chord_points() {
                prop_assert!(chord_point.distance_to(point) >= distance - 1e-6 * (1.0 + distance));
            }
        }

        #[test]
        fn a_similarity_moves_every_point_of_the_arc_with_it(ellipse in any_ellipse(), degrees in -180.0f64..180.0, factor in 0.2f64..5.0, f in 0.0f64..=1.0) {
            let transforms = [
                Transform::translation(Vec2::new(3.0, -4.0)),
                Transform::rotation(p(1.0, 2.0), degrees),
                Transform::scaling(p(1.0, 2.0), factor).unwrap(),
                Transform::mirror(p(0.0, 0.0), p(1.0, 1.0)).unwrap(),
            ];
            let point = ellipse.point_at(ellipse.start + ellipse.sweep * f);
            for transform in transforms {
                let image = ellipse.transformed(&transform);
                let expected = transform.apply(point);
                let tolerance = 1e-9 * (1.0 + expected.x.abs() + expected.y.abs());
                prop_assert!(image.distance_to_point(expected) < tolerance.max(1e-7), "{transform:?}: {expected:?} is {} off", image.distance_to_point(expected));
                prop_assert!((image.sweep - ellipse.sweep).abs() < 1e-9);
            }
        }
    }
}
