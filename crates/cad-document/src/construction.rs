//! Construction lines (#206; ADR-0031): an xline runs for ever both ways
//! through its base point, a ray for ever one way from its start. They
//! are entities like any other — in transactions, on layers, in DXF —
//! but have no bounds: they are kept out of the spatial index's tree,
//! do not count toward the extents, and are drawn clipped to the view.
//!
//! Every geometric question — picking aside — reads them as a segment
//! [`CONSTRUCTION_REACH`] long each way from the base point: what snaps
//! find, what they cut, and where they are cut.

use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{Segment, Transform};

/// How far, in drawing units, a construction line reaches from its base
/// point for every geometric question: a million units, past any drawing
/// drafted at a sensible unit, and short enough that a linear tolerance
/// around its ends stays a thousandth of a unit.
pub const CONSTRUCTION_REACH: f64 = 1.0e6;

/// An xline: a line through `base` along `direction`, endless both ways.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XLineEntity {
    pub base: Point2,
    /// A unit vector.
    pub direction: Vec2,
}

/// A ray: from `start` along `direction`, endless that way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayEntity {
    pub start: Point2,
    /// A unit vector.
    pub direction: Vec2,
}

/// The unit vector along `direction`, or `None` for none.
pub fn unit(direction: Vec2) -> Option<Vec2> {
    let length = direction.length();
    (length.is_finite() && length > 0.0).then(|| direction * (1.0 / length))
}

/// Where the infinite line through `base` along unit `direction` crosses
/// `bounds`, between parameters `from` and `to` along it: the stretch of
/// it inside, or `None`.
pub fn clip_to(
    base: Point2,
    direction: Vec2,
    from: f64,
    to: f64,
    bounds: &Aabb,
) -> Option<Segment> {
    let (mut lo, mut hi) = (from, to);
    for (origin, along, min, max) in [
        (base.x, direction.x, bounds.min().x, bounds.max().x),
        (base.y, direction.y, bounds.min().y, bounds.max().y),
    ] {
        if along == 0.0 {
            if origin < min || origin > max {
                return None;
            }
        } else {
            let (a, b) = ((min - origin) / along, (max - origin) / along);
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
        }
    }
    (lo <= hi).then(|| Segment::new(base + direction * lo, base + direction * hi))
}

/// Where two construction lines — each a base point, a unit direction,
/// and where along it it starts — cross, exactly; `None` when they are
/// parallel or the crossing is behind a ray's start.
pub fn crossing(a: (Point2, Vec2, f64), b: (Point2, Vec2, f64)) -> Option<Point2> {
    let ((pa, da, from_a), (pb, db, from_b)) = (a, b);
    let cross = da.x * db.y - da.y * db.x;
    if cross.abs() < 1e-12 {
        return None;
    }
    let between = pb - pa;
    let ta = (between.x * db.y - between.y * db.x) / cross;
    let tb = (between.x * da.y - between.y * da.x) / cross;
    (ta >= from_a && tb >= from_b).then(|| pa + da * ta)
}

impl XLineEntity {
    /// An xline through `base` along `direction`; `None` for no direction.
    pub fn new(base: Point2, direction: Vec2) -> Option<Self> {
        Some(Self {
            base,
            direction: unit(direction)?,
        })
    }

    /// The xline through two points; `None` when they coincide.
    pub fn through(base: Point2, through: Point2) -> Option<Self> {
        Self::new(base, through - base)
    }

    /// The xline as every geometric question reads it.
    pub fn segment(&self) -> Segment {
        Segment::new(
            self.base - self.direction * CONSTRUCTION_REACH,
            self.base + self.direction * CONSTRUCTION_REACH,
        )
    }

    /// Where along it a point lies, in units from the base.
    pub fn parameter_of(&self, point: Point2) -> f64 {
        (point - self.base).dot(self.direction)
    }

    pub fn distance_to(&self, point: Point2) -> f64 {
        let offset = point - self.base;
        (offset.x * self.direction.y - offset.y * self.direction.x).abs()
    }

    /// The part of it inside `bounds`.
    pub fn clipped(&self, bounds: &Aabb) -> Option<Segment> {
        clip_to(
            self.base,
            self.direction,
            f64::NEG_INFINITY,
            f64::INFINITY,
            bounds,
        )
    }

    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        let base = transform.apply(self.base);
        let ahead = transform.apply(self.base + self.direction);
        Self {
            base,
            direction: unit(ahead - base).unwrap_or(self.direction),
        }
    }
}

impl RayEntity {
    /// A ray from `start` along `direction`; `None` for no direction.
    pub fn new(start: Point2, direction: Vec2) -> Option<Self> {
        Some(Self {
            start,
            direction: unit(direction)?,
        })
    }

    /// The ray from `start` through `through`; `None` when they coincide.
    pub fn through(start: Point2, through: Point2) -> Option<Self> {
        Self::new(start, through - start)
    }

    /// The ray as every geometric question reads it.
    pub fn segment(&self) -> Segment {
        Segment::new(self.start, self.start + self.direction * CONSTRUCTION_REACH)
    }

    pub fn parameter_of(&self, point: Point2) -> f64 {
        (point - self.start).dot(self.direction)
    }

    pub fn distance_to(&self, point: Point2) -> f64 {
        let along = self.parameter_of(point);
        if along <= 0.0 {
            return self.start.distance_to(point);
        }
        let offset = point - self.start;
        (offset.x * self.direction.y - offset.y * self.direction.x).abs()
    }

    pub fn clipped(&self, bounds: &Aabb) -> Option<Segment> {
        clip_to(self.start, self.direction, 0.0, f64::INFINITY, bounds)
    }

    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        let start = transform.apply(self.start);
        let ahead = transform.apply(self.start + self.direction);
        Self {
            start,
            direction: unit(ahead - start).unwrap_or(self.direction),
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

    #[test]
    fn a_construction_line_is_clipped_to_a_box_and_measured_exactly() {
        let bounds = Aabb::from_points(p(0.0, 0.0), p(10.0, 10.0));
        let diagonal = XLineEntity::through(p(-5.0, -5.0), p(0.0, 0.0)).unwrap();
        let clipped = diagonal.clipped(&bounds).unwrap();
        assert!(clipped.start.distance_to(p(0.0, 0.0)) < 1e-9);
        assert!(clipped.end.distance_to(p(10.0, 10.0)) < 1e-9);
        let horizontal = XLineEntity::new(p(100.0, 5.0), Vec2::new(-3.0, 0.0)).unwrap();
        assert_eq!(horizontal.direction, Vec2::new(-1.0, 0.0));
        let clipped = horizontal.clipped(&bounds).unwrap();
        assert_eq!((clipped.start.y, clipped.end.y), (5.0, 5.0));
        assert!(
            XLineEntity::new(p(0.0, 20.0), Vec2::new(1.0, 0.0))
                .unwrap()
                .clipped(&bounds)
                .is_none()
        );
        assert!(XLineEntity::new(p(0.0, 0.0), Vec2::new(0.0, 0.0)).is_none());
        let ray = RayEntity::through(p(5.0, 5.0), p(20.0, 5.0)).unwrap();
        let clipped = ray.clipped(&bounds).unwrap();
        assert_eq!(clipped.start, p(5.0, 5.0));
        assert!(clipped.end.distance_to(p(10.0, 5.0)) < 1e-9);
        assert!(
            RayEntity::through(p(20.0, 5.0), p(30.0, 5.0))
                .unwrap()
                .clipped(&bounds)
                .is_none()
        );
        assert_eq!(
            ray.distance_to(p(0.0, 5.0)),
            5.0,
            "behind the start, from the start"
        );
        assert_eq!(ray.distance_to(p(50.0, 8.0)), 3.0);
        assert_eq!(diagonal.distance_to(p(1000.0, 1000.0)), 0.0);
    }

    proptest! {
        /// Every point of a clipped construction line is on the line and in
        /// the box; a transform keeps a point on the line on the line.
        #[test]
        fn clipped_pieces_lie_on_the_line_and_in_the_box(
            bx in -100.0..100.0_f64, by in -100.0..100.0_f64, angle in 0.0..std::f64::consts::TAU,
            x0 in -50.0..50.0_f64, y0 in -50.0..50.0_f64, w in 1.0..100.0_f64, h in 1.0..100.0_f64,
            turn in -360.0..360.0_f64,
        ) {
            let line = XLineEntity::new(p(bx, by), Vec2::from_polar(1.0, angle)).unwrap();
            let bounds = Aabb::from_points(p(x0, y0), p(x0 + w, y0 + h));
            if let Some(piece) = line.clipped(&bounds) {
                for point in [piece.start, piece.end] {
                    prop_assert!(line.distance_to(point) < 1e-7);
                    prop_assert!(bounds.expanded(1e-7).contains_point(point));
                }
            }
            let turned = line.transformed(&Transform::rotation(p(3.0, 4.0), turn));
            let on = line.base + line.direction * 37.0;
            prop_assert!(turned.distance_to(Transform::rotation(p(3.0, 4.0), turn).apply(on)) < 1e-7);
            prop_assert!((turned.direction.length() - 1.0).abs() < 1e-12);
        }
    }
}
