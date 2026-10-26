//! An opening through a wall (#325): a window or a door cut through a
//! wall drawn as two parallel faces — as a floor plan draws them, or as a
//! drafter does by hand — centred where asked, as wide as asked.
//!
//! The wall is the band between the faces; its centreline runs halfway
//! between them. The opening is a stretch of that centreline, and it must
//! lie where both faces are: an opening past a face's end would leave a
//! jamb in the air. What is drawn from it is pure geometry: the points
//! where each face is cut, the jambs across the wall at each end, a
//! window's glass as two lines along the wall's middle, and a door's leaf
//! — a thin rectangle standing into the room it opens into, hinged at the
//! end nearer the point that says which room — with its swing.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};

use crate::Segment;

/// Why there is no opening.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpeningError {
    /// The two faces are not parallel: not a wall.
    NotParallel,
    /// The faces lie on one line, or on top of each other: no wall between.
    NoThickness,
    /// The faces share no stretch: nothing is a wall between them.
    NoOverlap,
    /// The centre asked for is not on the wall's stretch between the faces.
    OffTheWall,
    /// The opening would reach past a face's end; how wide one fits
    /// there, centred where asked.
    TooWide { fits: f64 },
    /// A width that is not a size.
    BadWidth,
}

/// An opening through a wall, on its centreline from `from` to `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening {
    pub from: Point2,
    pub to: Point2,
    /// The wall's thickness: the distance between its faces.
    pub thickness: f64,
    /// Unit vector from `from` to `to`.
    pub along: Vec2,
    /// Unit vector across the wall, from the first face towards the second.
    pub across: Vec2,
}

/// A door's leaf drawn open, and its swing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    /// The leaf as a closed rectangle: the hinge, its far end, and the two
    /// corners its thickness adds, behind the hinge's side of the opening.
    pub outline: [Point2; 4],
    /// The swing's centre (the hinge), radius (the opening's width), and
    /// angles from the leaf to the other jamb, counter-clockwise.
    pub hinge: Point2,
    pub radius: f64,
    pub swing: (f64, f64),
}

impl Opening {
    /// The opening of `width` centred at the point of the wall's centreline
    /// nearest `centre`, the wall being the band between `first` and
    /// `second`. `tolerance` tells parallel from not, at the wall's size.
    pub fn new(
        first: Segment,
        second: Segment,
        centre: Point2,
        width: f64,
        tolerance: LinearTolerance,
    ) -> Result<Self, OpeningError> {
        if !(width.is_finite() && width > 0.0) {
            return Err(OpeningError::BadWidth);
        }
        let a = first.end - first.start;
        let b = second.end - second.start;
        let (la, lb) = (a.length(), b.length());
        if la <= tolerance.value() || lb <= tolerance.value() {
            return Err(OpeningError::NotParallel);
        }
        let along = a * (1.0 / la);
        let b_unit = b * (1.0 / lb);
        // Parallel: the sine of the angle between them is as good as zero
        // over the faces' own length.
        let sine = along.x * b_unit.y - along.y * b_unit.x;
        if sine.abs() * la.max(lb) > tolerance.value() * 1e3 {
            return Err(OpeningError::NotParallel);
        }
        let normal = Vec2::new(-along.y, along.x);
        let offset = (second.start - first.start).dot(normal);
        if offset.abs() <= tolerance.value() {
            return Err(OpeningError::NoThickness);
        }
        let across = normal * offset.signum();
        let thickness = offset.abs();
        // Both faces along the first's direction, and where they overlap.
        let t = |p: Point2| (p - first.start).dot(along);
        let (a0, a1) = (0.0_f64, la);
        let (b0, b1) = {
            let (s, e) = (t(second.start), t(second.end));
            (s.min(e), s.max(e))
        };
        let (lo, hi) = (a0.max(b0), a1.min(b1));
        if hi - lo <= tolerance.value() {
            return Err(OpeningError::NoOverlap);
        }
        let middle = t(centre);
        let away = ((centre - first.start).dot(normal) * offset.signum() - thickness / 2.0).abs();
        if middle < lo - tolerance.value() || middle > hi + tolerance.value() || away > thickness {
            return Err(OpeningError::OffTheWall);
        }
        let fits = 2.0 * (middle - lo).min(hi - middle);
        if width > fits + tolerance.value() {
            return Err(OpeningError::TooWide {
                fits: fits.max(0.0),
            });
        }
        let centreline = first.start + across * (thickness / 2.0);
        Ok(Self {
            from: centreline + along * (middle - width / 2.0),
            to: centreline + along * (middle + width / 2.0),
            thickness,
            along,
            across,
        })
    }

    /// How wide it is.
    pub fn width(&self) -> f64 {
        self.from.distance_to(self.to)
    }

    /// Where the first face is cut, and where the second: the stretch of
    /// each to take away.
    pub fn cuts(&self) -> [(Point2, Point2); 2] {
        let half = self.across * (self.thickness / 2.0);
        [
            (self.from - half, self.to - half),
            (self.from + half, self.to + half),
        ]
    }

    /// The jambs: a line across the wall at each end of the opening.
    pub fn jambs(&self) -> [(Point2, Point2); 2] {
        let half = self.across * (self.thickness / 2.0);
        [
            (self.from - half, self.from + half),
            (self.to - half, self.to + half),
        ]
    }

    /// A window's glass: two lines along the opening, a sixth of the
    /// wall's thickness either side of the centreline.
    pub fn glass(&self) -> [(Point2, Point2); 2] {
        let offset = self.across * (self.thickness / 6.0);
        [
            (self.from + offset, self.to + offset),
            (self.from - offset, self.to - offset),
        ]
    }

    /// A door's leaf, `leaf_thickness` thick, standing into the side of the
    /// wall `into` is on, hinged at the opening's end nearer `into`; and
    /// its swing from the leaf to the other jamb.
    pub fn leaf(&self, into: Point2, leaf_thickness: f64) -> Leaf {
        let side = if (into - self.from).dot(self.across) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let face = self.across * (side * self.thickness / 2.0);
        let (hinge_end, other_end, back) =
            if into.distance_to(self.from) <= into.distance_to(self.to) {
                (self.from, self.to, self.along * -leaf_thickness)
            } else {
                (self.to, self.from, self.along * leaf_thickness)
            };
        let hinge = hinge_end + face;
        let radius = self.width();
        let tip = hinge + self.across * (side * radius);
        let outline = [hinge, tip, tip + back, hinge + back];
        let angle = |p: Point2| (p.y - hinge.y).atan2(p.x - hinge.x);
        let (jamb, open) = (angle(other_end + face), angle(tip));
        let swing = if (open - jamb).rem_euclid(std::f64::consts::TAU)
            <= std::f64::consts::FRAC_PI_2 + 1e-9
        {
            (jamb, open)
        } else {
            (open, jamb)
        };
        Leaf {
            outline,
            hinge,
            radius,
            swing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn wall() -> (Segment, Segment) {
        // A horizontal wall 0.2 thick from x = 0 to 10, faces at y = 0 and 0.2.
        (
            Segment::new(p(0.0, 0.0), p(10.0, 0.0)),
            Segment::new(p(10.0, 0.2), p(0.0, 0.2)),
        )
    }

    const T: LinearTolerance = LinearTolerance::DEFAULT;

    #[test]
    fn an_opening_is_centred_on_the_wall_and_cuts_both_faces() {
        let (a, b) = wall();
        let o = Opening::new(a, b, p(4.0, 0.07), 1.5, T).unwrap();
        assert!((o.from.x - 3.25).abs() < 1e-9 && (o.to.x - 4.75).abs() < 1e-9);
        assert!((o.from.y - 0.1).abs() < 1e-9 && (o.thickness - 0.2).abs() < 1e-9);
        let [first, second] = o.cuts();
        assert!((first.0.y).abs() < 1e-9 && (second.0.y - 0.2).abs() < 1e-9);
        let [j0, j1] = o.jambs();
        assert!((j0.0.x - 3.25).abs() < 1e-9 && (j0.1.y - 0.2).abs() < 1e-9);
        assert!((j1.0.x - 4.75).abs() < 1e-9);
        for (s, e) in o.glass() {
            assert!((s.x - 3.25).abs() < 1e-9 && (e.x - 4.75).abs() < 1e-9);
            assert!((s.y - 0.1).abs() < 0.2 / 6.0 + 1e-9 && s.y > 0.0 && s.y < 0.2);
        }
        // The faces may be given either way round.
        let flipped = Opening::new(b, a, p(4.0, 0.07), 1.5, T).unwrap();
        assert!((flipped.width() - 1.5).abs() < 1e-9);
        assert!((flipped.from.y - 0.1).abs() < 1e-9);
    }

    #[test]
    fn what_is_not_a_wall_or_does_not_fit_is_refused() {
        let (a, b) = wall();
        let slanted = Segment::new(p(0.0, 0.2), p(10.0, 1.2));
        assert_eq!(
            Opening::new(a, slanted, p(5.0, 0.1), 1.0, T),
            Err(OpeningError::NotParallel)
        );
        assert_eq!(
            Opening::new(a, a, p(5.0, 0.0), 1.0, T),
            Err(OpeningError::NoThickness)
        );
        let apart = Segment::new(p(20.0, 0.2), p(30.0, 0.2));
        assert_eq!(
            Opening::new(a, apart, p(5.0, 0.1), 1.0, T),
            Err(OpeningError::NoOverlap)
        );
        assert_eq!(
            Opening::new(a, b, p(12.0, 0.1), 1.0, T),
            Err(OpeningError::OffTheWall)
        );
        assert_eq!(
            Opening::new(a, b, p(5.0, 3.0), 1.0, T),
            Err(OpeningError::OffTheWall)
        );
        assert_eq!(
            Opening::new(a, b, p(5.0, 0.1), 0.0, T),
            Err(OpeningError::BadWidth)
        );
        // Near the end only a narrow one fits, centred there.
        match Opening::new(a, b, p(0.5, 0.1), 1.5, T) {
            Err(OpeningError::TooWide { fits }) => assert!((fits - 1.0).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
        // The faces' overlap is what counts, not either face alone.
        let short = Segment::new(p(3.0, 0.2), p(6.0, 0.2));
        match Opening::new(a, short, p(5.5, 0.1), 2.0, T) {
            Err(OpeningError::TooWide { fits }) => assert!((fits - 1.0).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_leaf_stands_into_the_room_hinged_at_the_end_nearer_the_point() {
        let (a, b) = wall();
        let o = Opening::new(a, b, p(4.0, 0.1), 0.9, T).unwrap();
        // Into the room above, nearer the left end.
        let leaf = o.leaf(p(3.0, 2.0), 0.05);
        assert!((leaf.hinge.x - 3.55).abs() < 1e-9 && (leaf.hinge.y - 0.2).abs() < 1e-9);
        assert!(
            (leaf.outline[1].y - 1.1).abs() < 1e-9,
            "the leaf's end a width into the room"
        );
        assert!(
            (leaf.outline[2].x - 3.5).abs() < 1e-9,
            "its thickness behind the hinge"
        );
        assert!((leaf.radius - 0.9).abs() < 1e-9);
        let turn = (leaf.swing.1 - leaf.swing.0).rem_euclid(std::f64::consts::TAU);
        assert!((turn - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // Into the room below, nearer the right end.
        let leaf = o.leaf(p(5.0, -2.0), 0.05);
        assert!((leaf.hinge.x - 4.45).abs() < 1e-9 && leaf.hinge.y.abs() < 1e-9);
        assert!((leaf.outline[1].y + 0.9).abs() < 1e-9);
        assert!((leaf.outline[2].x - 4.5).abs() < 1e-9);
    }
}
