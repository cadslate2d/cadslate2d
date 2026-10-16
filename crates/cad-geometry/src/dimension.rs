//! Geometry a dimension is drawn with (DESIGN §34): arrowheads, the foot
//! of an extension line, the angle text reads at. Pure functions over
//! points; the layout that brings in a style and text metrics is in
//! `cad-document`.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};

use crate::Segment;

/// An arrowhead: a closed triangle with its tip on the line it ends and
/// its base `size` back along the line, a third as wide as it is long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrowhead {
    pub tip: Point2,
    /// The two corners of the base, either side of the line.
    pub base: [Point2; 2],
}

impl Arrowhead {
    /// Half the base, as a share of the length: the classic 1:6 taper.
    pub const HALF_WIDTH: f64 = 1.0 / 6.0;

    /// An arrowhead of `size` at `tip`, pointing along `direction`, a unit
    /// vector: its base lies `size` back against it.
    pub fn new(tip: Point2, direction: Vec2, size: f64) -> Self {
        let back = tip - direction * size;
        let side = perpendicular(direction) * (size * Self::HALF_WIDTH);
        Self {
            tip,
            base: [back + side, back - side],
        }
    }

    /// The middle of the base.
    fn heel(&self) -> Point2 {
        self.base[0] + (self.base[1] - self.base[0]) * 0.5
    }

    /// From the base to the tip.
    pub fn length(&self) -> f64 {
        self.tip.distance_to(self.heel())
    }

    /// The three corners: the tip, then the base.
    pub fn corners(&self) -> [Point2; 3] {
        [self.tip, self.base[0], self.base[1]]
    }

    /// The outline: tip to each base corner, and the base.
    pub fn segments(&self) -> [Segment; 3] {
        [
            Segment::new(self.tip, self.base[0]),
            Segment::new(self.base[0], self.base[1]),
            Segment::new(self.base[1], self.tip),
        ]
    }

    /// Whether `point` lies in the triangle, its edges included.
    pub fn contains(&self, point: Point2) -> bool {
        let [a, b, c] = self.corners();
        let side = |from: Point2, to: Point2| {
            let edge = to - from;
            let towards = point - from;
            edge.x * towards.y - edge.y * towards.x
        };
        let (ab, bc, ca) = (side(a, b), side(b, c), side(c, a));
        let all_left = ab >= 0.0 && bc >= 0.0 && ca >= 0.0;
        let all_right = ab <= 0.0 && bc <= 0.0 && ca <= 0.0;
        all_left || all_right
    }
}

/// `v` turned a quarter turn counter-clockwise.
pub fn perpendicular(v: Vec2) -> Vec2 {
    Vec2::new(-v.y, v.x)
}

/// `v` at unit length, or `None` for the zero vector (or one that is not
/// finite), which has no direction.
pub fn normalized(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    (length.is_finite() && length > 0.0).then(|| v * (1.0 / length))
}

/// The foot of the perpendicular from `point` to the line through
/// `through` along `direction`, a unit vector.
pub fn foot_on_line(point: Point2, through: Point2, direction: Vec2) -> Point2 {
    through + direction * (point - through).dot(direction)
}

/// The angle text reads at: `angle` or its opposite, whichever lies in
/// (−π/2, π/2], so that the text is never upside down.
pub fn readable_angle(angle: f64) -> f64 {
    let turned = angle.rem_euclid(std::f64::consts::PI);
    if turned > std::f64::consts::FRAC_PI_2 {
        turned - std::f64::consts::PI
    } else {
        turned
    }
}

/// The leader of an ordinate dimension (#227): from `offset` past the
/// feature towards `end` along `along` — a unit vector up the X axis's
/// leader or across the Y's — and to `end` itself. When `end` is not in
/// line with the feature the leader jogs across in its middle, over at
/// most `jog` of its length, so that it leaves the feature and reaches
/// the text square to the axis. A leader shorter than its offset is one
/// piece from the feature.
pub fn ordinate_leader(
    feature: Point2,
    end: Point2,
    along: Vec2,
    offset: f64,
    jog: f64,
) -> Vec<Segment> {
    let length = (end - feature).dot(along);
    if length <= offset {
        return vec![Segment::new(feature, end)];
    }
    let start = feature + along * offset;
    let across = (end - feature) - along * length;
    if LinearTolerance::DEFAULT
        .around(&[feature, end])
        .is_zero(across.length())
    {
        return vec![Segment::new(start, end)];
    }
    // The straight runs either side of the jog are the same length.
    let jog = jog.min((length - offset) / 3.0);
    let run = (length - offset - jog) / 2.0;
    let bend = start + along * run;
    let back = end - along * run;
    vec![
        Segment::new(start, bend),
        Segment::new(bend, back),
        Segment::new(back, end),
    ]
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    #[test]
    fn an_arrowhead_points_along_its_direction_with_its_base_behind() {
        let arrow = Arrowhead::new(p(10.0, 0.0), Vec2::new(1.0, 0.0), 3.0);
        assert!(close(arrow.base[0], p(7.0, 0.5)));
        assert!(close(arrow.base[1], p(7.0, -0.5)));
        assert!((arrow.length() - 3.0).abs() < 1e-12);
        assert!(arrow.contains(p(8.0, 0.1)));
        assert!(arrow.contains(p(10.0, 0.0)), "the tip is in");
        assert!(!arrow.contains(p(10.1, 0.0)), "past the tip is out");
        assert!(!arrow.contains(p(8.0, 0.4)), "beside the taper is out");
        assert_eq!(arrow.segments().len(), 3);
    }

    #[test]
    fn the_foot_is_the_projection_and_readable_angles_are_never_upside_down() {
        let foot = foot_on_line(p(3.0, 7.0), p(0.0, 2.0), Vec2::new(1.0, 0.0));
        assert!(close(foot, p(3.0, 2.0)));
        assert_eq!(readable_angle(0.0), 0.0);
        assert!((readable_angle(PI) - 0.0).abs() < 1e-12);
        assert!((readable_angle(FRAC_PI_2) - FRAC_PI_2).abs() < 1e-12);
        assert!((readable_angle(-FRAC_PI_2) - FRAC_PI_2).abs() < 1e-12);
        assert!((readable_angle(PI + FRAC_PI_4) - FRAC_PI_4).abs() < 1e-12);
        assert!((readable_angle(-FRAC_PI_4) + FRAC_PI_4).abs() < 1e-12);
        assert_eq!(normalized(Vec2::ZERO), None);
        assert!(close(
            Point2::ORIGIN + normalized(Vec2::new(0.0, -5.0)).unwrap(),
            p(0.0, -1.0)
        ));
    }

    #[test]
    fn an_ordinate_leader_runs_straight_or_jogs_across_in_its_middle() {
        let up = Vec2::new(0.0, 1.0);
        let straight = ordinate_leader(p(10.0, 0.0), p(10.0, 30.0), up, 2.0, 5.0);
        assert_eq!(straight, vec![Segment::new(p(10.0, 2.0), p(10.0, 30.0))]);
        let jogged = ordinate_leader(p(10.0, 0.0), p(20.0, 30.0), up, 2.0, 5.0);
        assert_eq!(
            jogged,
            vec![
                Segment::new(p(10.0, 2.0), p(10.0, 13.5)),
                Segment::new(p(10.0, 13.5), p(20.0, 18.5)),
                Segment::new(p(20.0, 18.5), p(20.0, 30.0)),
            ]
        );
        // Shorter than the offset: one piece from the feature.
        let short = ordinate_leader(p(0.0, 0.0), p(1.0, 1.0), up, 2.0, 5.0);
        assert_eq!(short, vec![Segment::new(p(0.0, 0.0), p(1.0, 1.0))]);
    }

    proptest! {
        #[test]
        fn an_ordinate_leader_is_connected_from_its_offset_to_its_end(
            x in -1000.0f64..1000.0, y in -1000.0f64..1000.0,
            dx in -50.0f64..50.0, dy in 0.0f64..200.0, horizontal in any::<bool>(),
        ) {
            let along = if horizontal { Vec2::new(1.0, 0.0) } else { Vec2::new(0.0, 1.0) };
            let feature = p(x, y);
            let end = feature + along * dy + perpendicular(along) * dx;
            let pieces = ordinate_leader(feature, end, along, 2.0, 5.0);
            prop_assert!(close(pieces.last().unwrap().end, end));
            for pair in pieces.windows(2) {
                prop_assert!(close(pair[0].end, pair[1].start));
            }
            if dy > 2.0 {
                prop_assert!(close(pieces[0].start, feature + along * 2.0));
            }
        }

        #[test]
        fn a_readable_angle_is_the_angle_or_its_opposite_within_a_right_angle(
            angle in -100.0f64..100.0,
        ) {
            let readable = readable_angle(angle);
            prop_assert!(readable > -FRAC_PI_2 - 1e-12 && readable <= FRAC_PI_2 + 1e-12);
            let difference = (readable - angle).rem_euclid(PI);
            prop_assert!(difference < 1e-9 || (PI - difference) < 1e-9);
        }

        #[test]
        fn an_arrowhead_contains_its_axis_and_not_what_lies_past_its_tip(
            x in -1000.0f64..1000.0, y in -1000.0f64..1000.0,
            angle in 0.0f64..std::f64::consts::TAU, size in 0.1f64..100.0,
            along in 0.0f64..1.0,
        ) {
            let direction = Vec2::from_polar(1.0, angle);
            let arrow = Arrowhead::new(p(x, y), direction, size);
            prop_assert!((arrow.length() - size).abs() < size * 1e-9);
            // Strictly inside the taper, from the heel to the tip.
            prop_assert!(arrow.contains(arrow.tip - direction * (size * along * 0.999)));
            prop_assert!(!arrow.contains(arrow.tip + direction * (size * 0.01)));
            prop_assert!(!arrow.contains(arrow.tip - direction * (size * 1.01)));
        }
    }
}
