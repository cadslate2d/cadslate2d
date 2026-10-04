//! What the modify commands can do to each kind of entity (DESIGN §22).
//!
//! M2 supports deliberately narrow cases; each operation here is a pure
//! function that either produces the result or says, as a value, why not.
//! The supported combinations are the published matrices in
//! `docs/modify-matrix.md`; a combination not listed there is an
//! `Unsupported` error, never a guess.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_geometry::{offset_circle, offset_segment};

use crate::{CircleEntity, Entity, LineEntity};

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// Why an entity could not be offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetError {
    /// This kind of entity cannot be offset yet; carries its name.
    Unsupported(&'static str),
    /// The side point is on the entity, so it chooses no side.
    NoSide,
    /// Offsetting inwards by this much would leave nothing.
    Collapses,
}

impl Entity {
    /// The kind of entity, as a lower-case noun for messages.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Entity::Line(_) => "line",
            Entity::Circle(_) => "circle",
            Entity::Arc(_) => "arc",
            Entity::Polyline(_) => "polyline",
        }
    }

    /// A copy of the entity `distance` away, on the side `side` is on: a
    /// parallel line, or a concentric circle.
    pub fn offset(&self, distance: f64, side: Point2) -> Result<Entity, OffsetError> {
        match self {
            Entity::Line(line) => offset_segment(&line.segment(), distance, side)
                .map(|parallel| LineEntity::new(parallel.start, parallel.end).into())
                .ok_or(OffsetError::NoSide),
            Entity::Circle(circle) => {
                let from_center = circle.center.distance_to(side);
                if TOLERANCE.eq(from_center, circle.radius) {
                    return Err(OffsetError::NoSide);
                }
                offset_circle(&circle.circle(), distance, side, TOLERANCE)
                    .map(|offset| CircleEntity::new(offset.center, offset.radius).into())
                    .ok_or(OffsetError::Collapses)
            }
            Entity::Arc(_) | Entity::Polyline(_) => Err(OffsetError::Unsupported(self.kind_name())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ArcEntity, PolylineEntity};

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn a_line_offsets_to_either_side() {
        let line: Entity = LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into();
        assert_eq!(
            line.offset(2.0, p(3.0, 9.0)),
            Ok(LineEntity::new(p(0.0, 2.0), p(10.0, 2.0)).into())
        );
        assert_eq!(
            line.offset(2.0, p(3.0, -9.0)),
            Ok(LineEntity::new(p(0.0, -2.0), p(10.0, -2.0)).into())
        );
        assert_eq!(line.offset(2.0, p(3.0, 0.0)), Err(OffsetError::NoSide));
    }

    #[test]
    fn a_circle_offsets_in_either_direction_but_not_to_nothing() {
        let circle: Entity = CircleEntity::new(p(0.0, 0.0), 10.0).into();
        assert_eq!(
            circle.offset(4.0, p(50.0, 0.0)),
            Ok(CircleEntity::new(p(0.0, 0.0), 14.0).into())
        );
        assert_eq!(
            circle.offset(4.0, p(1.0, 0.0)),
            Ok(CircleEntity::new(p(0.0, 0.0), 6.0).into())
        );
        assert_eq!(
            circle.offset(10.0, p(1.0, 0.0)),
            Err(OffsetError::Collapses)
        );
        assert_eq!(
            circle.offset(25.0, p(1.0, 0.0)),
            Err(OffsetError::Collapses)
        );
        assert_eq!(circle.offset(4.0, p(10.0, 0.0)), Err(OffsetError::NoSide));
    }

    #[test]
    fn arcs_and_polylines_are_not_supported_yet_and_say_so() {
        let arc: Entity = ArcEntity::new(p(0.0, 0.0), 5.0, 0.0, 1.0).into();
        let polyline: Entity = PolylineEntity::new(vec![p(0.0, 0.0), p(1.0, 0.0)], false).into();
        assert_eq!(
            arc.offset(1.0, p(9.0, 9.0)),
            Err(OffsetError::Unsupported("arc"))
        );
        assert_eq!(
            polyline.offset(1.0, p(9.0, 9.0)),
            Err(OffsetError::Unsupported("polyline"))
        );
    }
}
