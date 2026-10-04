//! The transforms that editing commands apply to geometry (DESIGN §22):
//! move, rotate, uniform scale, and mirror.
//!
//! All four are similarities, so every primitive keeps its kind: a line
//! stays a line, a circle a circle, an arc an arc. Each is computed in the
//! most direct way rather than through a general matrix, so the common exact
//! cases stay exact — a translation adds and nothing else, a quarter turn
//! swaps coordinates, a mirror across an axis-parallel line negates one.

use std::f64::consts::FRAC_PI_2;

use cad_core::tolerance::AngularTolerance;
use cad_core::{Point2, Vec2};

use crate::{Arc, normalize_angle};

/// The unit vector at `degrees` counter-clockwise from +X. Multiples of a
/// quarter turn are exact: no `6e-17` where a zero belongs.
pub fn unit_at_degrees(degrees: f64) -> Vec2 {
    let quarters = degrees / 90.0;
    let nearest = quarters.round();
    if AngularTolerance::DEFAULT.is_zero((quarters - nearest) * FRAC_PI_2) {
        const AXES: [(f64, f64); 4] = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)];
        let (x, y) = AXES[nearest.rem_euclid(4.0) as usize];
        return Vec2::new(x, y);
    }
    Vec2::from_polar(1.0, degrees.to_radians())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Translate(Vec2),
    Rotate {
        about: Point2,
        /// `(cos, sin)` of the rotation.
        turn: Vec2,
        radians: f64,
    },
    Scale {
        about: Point2,
        factor: f64,
    },
    Mirror {
        /// A point on the mirror line.
        on: Point2,
        /// The unit direction of the mirror line.
        axis: Vec2,
        /// Twice the direction angle of the line: a direction at `φ` is
        /// reflected to `2α − φ`.
        twice_axis_angle: f64,
    },
}

/// A move, a rotation, a uniform scaling, or a reflection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(Kind);

impl Transform {
    /// Moves everything by `offset`.
    pub fn translation(offset: Vec2) -> Self {
        Self(Kind::Translate(offset))
    }

    /// Rotates by `degrees` counter-clockwise about `about`.
    pub fn rotation(about: Point2, degrees: f64) -> Self {
        Self(Kind::Rotate {
            about,
            turn: unit_at_degrees(degrees),
            radians: degrees.to_radians(),
        })
    }

    /// Scales uniformly by `factor` about `about`. `None` unless the factor
    /// is finite and greater than zero.
    pub fn scaling(about: Point2, factor: f64) -> Option<Self> {
        (factor.is_finite() && factor > 0.0).then_some(Self(Kind::Scale { about, factor }))
    }

    /// Reflects across the line through `from` and `to`. `None` when the two
    /// points coincide and define no line.
    pub fn mirror(from: Point2, to: Point2) -> Option<Self> {
        let along = to - from;
        let length = along.length();
        (length.is_finite() && length > 0.0).then(|| {
            Self(Kind::Mirror {
                on: from,
                axis: along * (1.0 / length),
                twice_axis_angle: 2.0 * along.y.atan2(along.x),
            })
        })
    }

    pub fn apply(&self, point: Point2) -> Point2 {
        match self.0 {
            Kind::Translate(offset) => point + offset,
            Kind::Rotate { about, turn, .. } => {
                let from = point - about;
                let rotated = Vec2::new(
                    from.x * turn.x - from.y * turn.y,
                    from.x * turn.y + from.y * turn.x,
                );
                about + rotated
            }
            Kind::Scale { about, factor } => about + (point - about) * factor,
            Kind::Mirror { on, axis, .. } => {
                let from = point - on;
                on + (axis * (2.0 * from.dot(axis)) - from)
            }
        }
    }

    /// What lengths — a radius, say — are multiplied by.
    pub fn scale_factor(&self) -> f64 {
        match self.0 {
            Kind::Scale { factor, .. } => factor,
            Kind::Translate(_) | Kind::Rotate { .. } | Kind::Mirror { .. } => 1.0,
        }
    }

    /// Where a direction at `angle` (radians from +X) points afterwards.
    /// Unchanged, bit for bit, by a move or a scaling.
    pub fn apply_angle(&self, angle: f64) -> f64 {
        match self.0 {
            Kind::Translate(_) | Kind::Scale { .. } => angle,
            Kind::Rotate { radians, .. } => normalize_angle(angle + radians),
            Kind::Mirror {
                twice_axis_angle, ..
            } => normalize_angle(twice_axis_angle - angle),
        }
    }

    /// True for a mirror: counter-clockwise becomes clockwise.
    pub fn reverses_orientation(&self) -> bool {
        matches!(self.0, Kind::Mirror { .. })
    }

    /// The image of an arc. Arcs are counter-clockwise by definition, so
    /// under a mirror the image starts where the original ended.
    pub fn apply_arc(&self, arc: &Arc) -> Arc {
        let start = if self.reverses_orientation() {
            arc.start + arc.sweep
        } else {
            arc.start
        };
        Arc::new(
            self.apply(arc.center),
            arc.radius * self.scale_factor(),
            self.apply_angle(start),
            arc.sweep,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9 * (1.0 + a.distance_to(Point2::ORIGIN))
    }

    #[test]
    fn unit_vectors_on_quarter_turns_are_exact() {
        assert_eq!(unit_at_degrees(0.0), Vec2::new(1.0, 0.0));
        assert_eq!(unit_at_degrees(90.0), Vec2::new(0.0, 1.0));
        assert_eq!(unit_at_degrees(180.0), Vec2::new(-1.0, 0.0));
        assert_eq!(unit_at_degrees(270.0), Vec2::new(0.0, -1.0));
        assert_eq!(unit_at_degrees(-90.0), Vec2::new(0.0, -1.0));
        assert_eq!(unit_at_degrees(720.0), Vec2::new(1.0, 0.0));
        let diagonal = unit_at_degrees(45.0);
        assert!((diagonal.x - diagonal.y).abs() < 1e-15 && (diagonal.length() - 1.0).abs() < 1e-15);
    }

    #[test]
    fn a_translation_only_adds() {
        let transform = Transform::translation(Vec2::new(0.1, -0.2));
        assert_eq!(
            transform.apply(p(1.0, 2.0)),
            p(1.0, 2.0) + Vec2::new(0.1, -0.2)
        );
        assert_eq!(transform.scale_factor(), 1.0);
        assert_eq!(
            transform.apply_angle(7.5),
            7.5,
            "angles pass through untouched"
        );
        assert!(!transform.reverses_orientation());
    }

    #[test]
    fn quarter_turns_are_exact() {
        let about = p(10.0, 20.0);
        assert_eq!(
            Transform::rotation(about, 90.0).apply(p(13.0, 20.0)),
            p(10.0, 23.0)
        );
        assert_eq!(
            Transform::rotation(about, 180.0).apply(p(13.0, 24.0)),
            p(7.0, 16.0)
        );
        assert_eq!(
            Transform::rotation(about, -90.0).apply(p(13.0, 20.0)),
            p(10.0, 17.0)
        );
        assert_eq!(
            Transform::rotation(about, 360.0).apply(p(13.5, 24.25)),
            p(13.5, 24.25)
        );
    }

    #[test]
    fn a_rotation_turns_points_and_directions_counter_clockwise() {
        let transform = Transform::rotation(p(0.0, 0.0), 30.0);
        let image = transform.apply(p(2.0, 0.0));
        assert!(close(image, p(3f64.sqrt(), 1.0)));
        assert!((transform.apply_angle(0.25) - (0.25 + PI / 6.0)).abs() < 1e-15);
        assert!(
            (transform.apply_angle(TAU - 0.1) - (PI / 6.0 - 0.1)).abs() < 1e-12,
            "wraps"
        );
    }

    #[test]
    fn scaling_multiplies_distances_from_the_base_point() {
        let transform = Transform::scaling(p(10.0, 10.0), 3.0).unwrap();
        assert_eq!(transform.apply(p(12.0, 9.0)), p(16.0, 7.0));
        assert_eq!(transform.apply(p(10.0, 10.0)), p(10.0, 10.0));
        assert_eq!(transform.scale_factor(), 3.0);
        assert_eq!(transform.apply_angle(1.25), 1.25);
    }

    #[test]
    fn a_scale_factor_must_be_positive_and_finite() {
        for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(Transform::scaling(Point2::ORIGIN, factor), None, "{factor}");
        }
    }

    #[test]
    fn mirrors_across_axis_parallel_lines_are_exact() {
        let vertical = Transform::mirror(p(5.0, 0.0), p(5.0, 1.0)).unwrap();
        assert_eq!(vertical.apply(p(2.0, 7.5)), p(8.0, 7.5));
        let horizontal = Transform::mirror(p(0.0, -1.0), p(9.0, -1.0)).unwrap();
        assert_eq!(horizontal.apply(p(2.0, 7.5)), p(2.0, -9.5));
        assert!(vertical.reverses_orientation());
    }

    #[test]
    fn a_mirror_reflects_points_and_directions() {
        // Across the diagonal y = x: coordinates swap.
        let diagonal = Transform::mirror(p(0.0, 0.0), p(1.0, 1.0)).unwrap();
        assert!(close(diagonal.apply(p(3.0, 1.0)), p(1.0, 3.0)));
        // A direction along +X ends up along +Y.
        assert!((diagonal.apply_angle(0.0) - FRAC_PI_2).abs() < 1e-15);
        // Points on the line stay put.
        assert!(close(diagonal.apply(p(4.0, 4.0)), p(4.0, 4.0)));
    }

    #[test]
    fn a_mirror_line_needs_two_distinct_points() {
        assert_eq!(Transform::mirror(p(1.0, 1.0), p(1.0, 1.0)), None);
        assert_eq!(Transform::mirror(p(1.0, 1.0), p(f64::NAN, 1.0)), None);
    }

    #[test]
    fn a_rotated_arc_keeps_its_sweep_and_turns_its_start() {
        let arc = Arc::new(p(10.0, 0.0), 5.0, 0.0, FRAC_PI_2);
        let image = Transform::rotation(p(0.0, 0.0), 90.0).apply_arc(&arc);
        assert_eq!(image.center, p(0.0, 10.0));
        assert_eq!((image.radius, image.sweep), (5.0, FRAC_PI_2));
        assert!((image.start - FRAC_PI_2).abs() < 1e-15);
        assert!(close(image.start_point(), p(0.0, 15.0)));
        assert!(close(image.end_point(), p(-5.0, 10.0)));
    }

    /// Arcs are counter-clockwise; a mirror reverses direction, so the image
    /// starts at the image of the old end.
    #[test]
    fn a_mirrored_arc_is_still_counter_clockwise_and_covers_the_mirrored_points() {
        let arc = Arc::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2); // (10,0) → (0,10)
        let mirror = Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap(); // across the Y axis
        let image = mirror.apply_arc(&arc);
        assert_eq!(image.sweep, FRAC_PI_2);
        assert!(close(image.start_point(), p(0.0, 10.0)));
        assert!(close(image.end_point(), p(-10.0, 0.0)));
        assert!(close(image.mid_point(), mirror.apply(arc.mid_point())));
    }

    #[test]
    fn a_scaled_arc_scales_its_radius_and_keeps_its_angles() {
        let arc = Arc::new(p(4.0, 0.0), 2.0, 1.0, 0.5);
        let image = Transform::scaling(p(0.0, 0.0), 2.5)
            .unwrap()
            .apply_arc(&arc);
        assert_eq!(image, Arc::new(p(10.0, 0.0), 5.0, 1.0, 0.5));
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -1e4..1e4f64
    }

    fn point() -> impl Strategy<Value = Point2> {
        (coordinate(), coordinate()).prop_map(|(x, y)| Point2::new(x, y))
    }

    proptest! {
        #[test]
        fn rotating_there_and_back_is_the_identity(about in point(), at in point(), degrees in -720.0..720f64) {
            let there = Transform::rotation(about, degrees);
            let back = Transform::rotation(about, -degrees);
            prop_assert!(back.apply(there.apply(at)).distance_to(at) < 1e-7);
        }

        #[test]
        fn rotation_preserves_distances(about in point(), a in point(), b in point(), degrees in -720.0..720f64) {
            let transform = Transform::rotation(about, degrees);
            let before = a.distance_to(b);
            let after = transform.apply(a).distance_to(transform.apply(b));
            prop_assert!((before - after).abs() < 1e-7);
        }

        #[test]
        fn mirroring_twice_is_the_identity(from in point(), to in point(), at in point()) {
            prop_assume!(from.distance_to(to) > 1e-3);
            let mirror = Transform::mirror(from, to).unwrap();
            prop_assert!(mirror.apply(mirror.apply(at)).distance_to(at) < 1e-6);
        }

        #[test]
        fn mirroring_an_angle_twice_is_the_identity(from in point(), to in point(), angle in 0.0..TAU) {
            prop_assume!(from.distance_to(to) > 1e-3);
            let mirror = Transform::mirror(from, to).unwrap();
            let twice = mirror.apply_angle(mirror.apply_angle(angle));
            let difference = (twice - angle).abs();
            prop_assert!(difference.min(TAU - difference) < 1e-9);
        }

        #[test]
        fn scaling_multiplies_every_distance(about in point(), a in point(), b in point(), factor in 0.01..100f64) {
            let transform = Transform::scaling(about, factor).unwrap();
            let before = a.distance_to(b);
            let after = transform.apply(a).distance_to(transform.apply(b));
            prop_assert!((after - before * factor).abs() < 1e-6 * (1.0 + before * factor));
        }

        /// The image of a point on an arc lies on the image of the arc, for
        /// every kind of transform.
        #[test]
        fn arc_images_carry_their_points(
            center in point(), radius in 0.1..500f64, start in 0.0..TAU, sweep in 0.01..TAU,
            about in point(), other in point(), degrees in -360.0..360f64, factor in 0.1..10f64,
            along in 0.0..1f64,
        ) {
            prop_assume!(about.distance_to(other) > 1e-3);
            let arc = Arc::new(center, radius, start, sweep);
            let on_arc = arc.point_at(start + sweep * along);
            let transforms = [
                Transform::translation(other - about),
                Transform::rotation(about, degrees),
                Transform::scaling(about, factor).unwrap(),
                Transform::mirror(about, other).unwrap(),
            ];
            for transform in transforms {
                let image = transform.apply_arc(&arc);
                let distance = image.distance_to_point(transform.apply(on_arc));
                prop_assert!(distance < 1e-6 * (1.0 + image.radius), "{transform:?}: off by {distance}");
            }
        }
    }
}
