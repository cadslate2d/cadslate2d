//! What `SPLINEDIT` does to a spline (#237): closed or opened, reversed,
//! a fit point added or deleted, or turned into a polyline. Pure
//! functions; the command applies them in transactions.
//!
//! A spline drawn through points is changed through them and fitted
//! again; one given by control points alone has its control polygon
//! changed, with knots spaced evenly. Closing a spline through points
//! fits it through its first point again, so it meets itself; the
//! joint is where it began.

use std::fmt;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::{PolylineEntity, SplineEntity};

/// Why a spline cannot be edited so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplineEditError {
    AlreadyClosed,
    AlreadyOpen,
    /// The spline has control points only: no fit points to edit.
    NoFitData,
    /// Fewer points would be left than a spline needs.
    TooFewPoints,
    /// No spline goes through the points so: two in a row the same.
    NoSpline,
}

impl fmt::Display for SplineEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AlreadyClosed => "The spline is closed already.",
            Self::AlreadyOpen => "The spline is open already.",
            Self::NoFitData => "The spline has control points only: it has no fit points to edit.",
            Self::TooFewPoints => "A spline needs more points than that.",
            Self::NoSpline => "No spline goes through those points.",
        })
    }
}

impl std::error::Error for SplineEditError {}

/// Knots for `count` control points of `degree`, clamped at both ends and
/// evenly spaced between, over [0, 1].
fn even_knots(count: usize, degree: usize) -> Vec<f64> {
    let inner = count - degree;
    (0..count + degree + 1)
        .map(|index| {
            if index <= degree {
                0.0
            } else if index >= count {
                1.0
            } else {
                (index - degree) as f64 / inner as f64
            }
        })
        .collect()
}

impl SplineEntity {
    /// Through its fit points, or from its control points, as given.
    fn rebuilt(&self, points: Vec<Point2>, weights: Vec<f64>) -> Result<Self, SplineEditError> {
        if !self.fit.is_empty() {
            return Self::through(&points).ok_or(SplineEditError::NoSpline);
        }
        if points.len() <= self.degree {
            return Err(SplineEditError::TooFewPoints);
        }
        let spline = Self {
            knots: even_knots(points.len(), self.degree),
            control: points,
            weights,
            ..self.clone()
        };
        if spline.is_valid() {
            Ok(spline)
        } else {
            Err(SplineEditError::NoSpline)
        }
    }

    /// The spline made to meet itself where it began.
    pub fn closed(&self) -> Result<Self, SplineEditError> {
        if self.is_closed() {
            return Err(SplineEditError::AlreadyClosed);
        }
        let mut points = self.defining_points().to_vec();
        points.push(points[0]);
        let mut weights = self.weights.clone();
        if let Some(first) = weights.first().copied() {
            weights.push(first);
        }
        self.rebuilt(points, weights)
    }

    /// The closed spline opened where it began: its last point, which is
    /// its first again, taken away.
    pub fn opened(&self) -> Result<Self, SplineEditError> {
        if !self.is_closed() {
            return Err(SplineEditError::AlreadyOpen);
        }
        let mut points = self.defining_points().to_vec();
        let tolerance = LinearTolerance::DEFAULT.around(&points);
        if points.len() < 3 || !tolerance.coincident(points[0], points[points.len() - 1]) {
            return Err(SplineEditError::TooFewPoints);
        }
        points.pop();
        let mut weights = self.weights.clone();
        weights.pop();
        self.rebuilt(points, weights)
    }

    /// The same curve run the other way: its points, weights, and knots
    /// reversed, so that nothing is fitted again.
    #[must_use]
    pub fn reversed(&self) -> Self {
        let (start, end) = self.spline().domain();
        Self {
            degree: self.degree,
            control: self.control.iter().rev().copied().collect(),
            knots: self
                .knots
                .iter()
                .rev()
                .map(|knot| start + end - knot)
                .collect(),
            weights: self.weights.iter().rev().copied().collect(),
            fit: self.fit.iter().rev().copied().collect(),
        }
    }

    /// The fit point nearest `point`.
    pub fn nearest_fit_point(&self, point: Point2) -> Option<usize> {
        self.fit
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.distance_to(point).total_cmp(&b.distance_to(point)))
            .map(|(index, _)| index)
    }

    /// The spline fitted through a new point after fit point `after`.
    pub fn with_fit_point(&self, after: usize, point: Point2) -> Result<Self, SplineEditError> {
        if self.fit.is_empty() {
            return Err(SplineEditError::NoFitData);
        }
        let mut points = self.fit.clone();
        points.insert((after + 1).min(points.len()), point);
        self.rebuilt(points, Vec::new())
    }

    /// The spline fitted through its fit points but `index`.
    pub fn without_fit_point(&self, index: usize) -> Result<Self, SplineEditError> {
        if self.fit.is_empty() {
            return Err(SplineEditError::NoFitData);
        }
        if self.fit.len() <= 2 || index >= self.fit.len() {
            return Err(SplineEditError::TooFewPoints);
        }
        let mut points = self.fit.clone();
        points.remove(index);
        self.rebuilt(points, Vec::new())
    }

    /// The curve as a polyline of its chords, closed when it is.
    pub fn to_polyline(&self) -> PolylineEntity {
        let mut points = self.spline().chord_points();
        let closed = self.is_closed();
        if closed {
            points.pop();
        }
        PolylineEntity::new(points, closed)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use cad_geometry::Spline;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn wave() -> SplineEntity {
        SplineEntity::through(&[p(0.0, 0.0), p(10.0, 10.0), p(20.0, 0.0), p(30.0, 10.0)]).unwrap()
    }

    #[test]
    fn a_spline_closes_and_opens_again() {
        let closed = wave().closed().unwrap();
        assert!(closed.is_closed());
        assert_eq!(closed.closed(), Err(SplineEditError::AlreadyClosed));
        let opened = closed.opened().unwrap();
        assert!(!opened.is_closed());
        assert_eq!(opened.fit, wave().fit);
        assert_eq!(wave().opened(), Err(SplineEditError::AlreadyOpen));
        // Control points only: the control polygon closes.
        let control = SplineEntity::from_spline(Spline {
            degree: 2,
            control: vec![p(0.0, 0.0), p(10.0, 10.0), p(20.0, 0.0)],
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            weights: Vec::new(),
        });
        let closed = control.closed().unwrap();
        assert!(closed.is_closed());
        assert_eq!(closed.control.len(), 4);
        assert_eq!(
            control.with_fit_point(0, p(1.0, 1.0)),
            Err(SplineEditError::NoFitData)
        );
    }

    #[test]
    fn fit_points_are_added_and_deleted_and_a_polyline_follows_the_curve() {
        let added = wave().with_fit_point(1, p(15.0, 8.0)).unwrap();
        assert_eq!(added.fit[2], p(15.0, 8.0));
        assert_eq!(added.fit.len(), 5);
        let deleted = added.without_fit_point(2).unwrap();
        assert_eq!(deleted.fit, wave().fit);
        let two = SplineEntity::through(&[p(0.0, 0.0), p(1.0, 1.0)]).unwrap();
        assert_eq!(two.without_fit_point(0), Err(SplineEditError::TooFewPoints));
        assert_eq!(wave().nearest_fit_point(p(19.0, 1.0)), Some(2));
        let polyline = wave().to_polyline();
        assert!(!polyline.closed);
        assert_eq!(polyline.vertices[0], p(0.0, 0.0));
        let closed = wave().closed().unwrap().to_polyline();
        assert!(closed.closed);
    }

    proptest! {
        /// Reversed twice, a spline is itself; reversed once, it is the
        /// same curve run the other way; its polyline lies on it.
        #[test]
        fn reversing_and_polylines_keep_the_curve(
            points in prop::collection::vec((-100.0f64..100.0, -100.0f64..100.0), 2..8),
        ) {
            let points: Vec<Point2> = points.into_iter().map(|(x, y)| p(x, y)).collect();
            let Some(spline) = SplineEntity::through(&points) else {
                return Ok(());
            };
            let twice = spline.reversed().reversed();
            prop_assert_eq!(&twice.control, &spline.control);
            prop_assert_eq!(&twice.fit, &spline.fit);
            for (a, b) in twice.knots.iter().zip(&spline.knots) {
                prop_assert!((a - b).abs() < 1e-12);
            }
            let (forward, backward) = (spline.spline(), spline.reversed().spline());
            let (start, end) = forward.domain();
            for t in [0.0, 0.3, 0.5, 0.8, 1.0] {
                let at = start + (end - start) * t;
                let there = backward.point_at(start + end - at);
                prop_assert!(forward.point_at(at).distance_to(there) < 1e-6, "{t}");
            }
            let size = 200.0;
            for vertex in spline.to_polyline().vertices {
                prop_assert!(forward.distance_to_point(vertex) < 1e-3 * size);
            }
        }
    }
}
