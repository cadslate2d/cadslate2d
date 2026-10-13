//! Reading a SPLINE (#182), and a hatch's spline edge.
//!
//! A SPLINE's groups are lists — knots (40), weights (41), control points
//! (10, 20), fit points (11, 21) — so, like a HATCH's, they are kept in
//! order and read by walking them. A spline with fit points and no
//! control data is interpolated through them; one with both keeps both.

use cad_core::Point2;
use cad_document::{Spline, SplineEntity};

/// The groups of one record, in order.
pub(crate) struct Groups<'a>(pub &'a [(i32, String)]);

impl Groups<'_> {
    fn reals(&self, code: i32) -> Vec<f64> {
        self.0
            .iter()
            .filter(|(c, _)| *c == code)
            .filter_map(|(_, v)| v.trim().parse::<f64>().ok())
            .collect()
    }

    fn int(&self, code: i32) -> Option<i64> {
        self.0
            .iter()
            .find(|(c, _)| *c == code)
            .and_then(|(_, v)| v.trim().parse().ok())
    }

    /// The points whose x is `x` and y is `x + 10`, paired in order.
    fn points(&self, x: i32) -> Vec<Point2> {
        let xs = self.reals(x);
        let ys = self.reals(x + 10);
        xs.into_iter()
            .zip(ys)
            .map(|(px, py)| Point2::new(px, py))
            .collect()
    }
}

/// Builds the entity from what was read, `None` when it is not a spline.
pub(crate) fn assemble(
    degree: usize,
    control: Vec<Point2>,
    knots: Vec<f64>,
    weights: Vec<f64>,
    fit: Vec<Point2>,
) -> Option<SplineEntity> {
    let from_fit = || SplineEntity::through(&fit);
    if control.is_empty() {
        return from_fit();
    }
    let spline = Spline {
        degree,
        control,
        knots,
        weights: if weights.iter().all(|w| (*w - 1.0).abs() < 1e-12) {
            Vec::new()
        } else {
            weights
        },
    };
    if !spline.is_valid() {
        return from_fit();
    }
    let entity = SplineEntity {
        fit,
        ..SplineEntity::from_spline(spline)
    };
    entity.is_valid().then_some(entity)
}

/// Reads a SPLINE record.
pub(crate) fn read(groups: &[(i32, String)]) -> Option<SplineEntity> {
    let g = Groups(groups);
    let degree = g.int(71).unwrap_or(3).max(1) as usize;
    assemble(degree, g.points(10), g.reals(40), g.reals(41), g.points(11))
}
