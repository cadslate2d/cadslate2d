//! Centralized tolerances (DESIGN §21.1).
//!
//! There is no global `EPSILON`. Each comparison names the *class* of tolerance
//! it needs, and the classes are distinct types so one cannot be passed where
//! another is expected.
//!
//! | Class | Unit | Use for | Do not use for |
//! | --- | --- | --- | --- |
//! | [`LinearTolerance`] | drawing units | point coincidence, zero-length checks, point-on-curve distance | pixels, angles, curve parameters |
//! | [`AngularTolerance`] | radians | parallelism, angle equality | lengths |
//! | [`ParametricTolerance`] | dimensionless | equality of parameters along a curve (trim/extend) | lengths, angles |
//!
//! Two further classes live elsewhere on purpose:
//!
//! - **Display/picking tolerance** is measured in pixels and belongs to the
//!   view (`cad_core::View`), because it must stay constant on screen while
//!   zooming (§18.3).
//! - **File-import tolerance** is reserved for `cad-dxf` cleanup of imported
//!   geometry; nothing uses it yet.
//!
//! Orientation and other topological decisions should use robust predicates,
//! not a tolerance (§21.2).

use std::marker::PhantomData;

use crate::Point2;

/// A non-negative tolerance value tagged with its class.
pub struct Tolerance<Class> {
    value: f64,
    class: PhantomData<Class>,
}

/// Marker for [`LinearTolerance`].
#[derive(Debug)]
pub enum Linear {}
/// Marker for [`AngularTolerance`].
#[derive(Debug)]
pub enum Angular {}
/// Marker for [`ParametricTolerance`].
#[derive(Debug)]
pub enum Parametric {}

/// Geometric distance tolerance, in drawing units.
pub type LinearTolerance = Tolerance<Linear>;
/// Angle tolerance, in radians. Does not wrap: normalise angles first.
pub type AngularTolerance = Tolerance<Angular>;
/// Tolerance on a dimensionless curve parameter.
pub type ParametricTolerance = Tolerance<Parametric>;

impl<Class> Tolerance<Class> {
    /// # Panics
    /// Panics if `value` is negative or not finite.
    pub const fn new(value: f64) -> Self {
        assert!(
            value >= 0.0 && value.is_finite(),
            "tolerance must be finite and non-negative"
        );
        Self {
            value,
            class: PhantomData,
        }
    }

    pub const fn value(self) -> f64 {
        self.value
    }

    /// True when `magnitude` is indistinguishable from zero.
    pub fn is_zero(self, magnitude: f64) -> bool {
        magnitude.abs() <= self.value
    }

    /// True when `a` and `b` are indistinguishable.
    pub fn eq(self, a: f64, b: f64) -> bool {
        self.is_zero(a - b)
    }
}

impl LinearTolerance {
    /// Default geometric tolerance for document coordinates.
    pub const DEFAULT: Self = Self::new(1e-9);

    /// True when two points are the same point for drafting purposes.
    pub fn coincident(self, a: Point2, b: Point2) -> bool {
        self.is_zero(a.distance_to(b))
    }
}

impl AngularTolerance {
    pub const DEFAULT: Self = Self::new(1e-9);
}

impl ParametricTolerance {
    pub const DEFAULT: Self = Self::new(1e-12);
}

// Manual impls: derives would add bounds on the uninhabited marker type.
impl<Class> Clone for Tolerance<Class> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Class> Copy for Tolerance<Class> {}
impl<Class> std::fmt::Debug for Tolerance<Class> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tolerance({})", self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_within_tolerance_are_equal() {
        let tol = LinearTolerance::new(0.01);
        assert!(tol.eq(1.0, 1.005));
        assert!(tol.eq(1.005, 1.0));
        assert!(!tol.eq(1.0, 1.02));
    }

    #[test]
    fn zero_check_is_symmetric_in_sign() {
        let tol = AngularTolerance::new(1e-6);
        assert!(tol.is_zero(5e-7));
        assert!(tol.is_zero(-5e-7));
        assert!(!tol.is_zero(2e-6));
    }

    #[test]
    fn coincident_points() {
        let tol = LinearTolerance::DEFAULT;
        let p = Point2::new(100.0, 200.0);
        assert!(tol.coincident(p, p));
        assert!(tol.coincident(p, Point2::new(100.0 + 1e-10, 200.0)));
        assert!(!tol.coincident(p, Point2::new(100.001, 200.0)));
    }

    #[test]
    fn defaults_are_distinct_classes_with_documented_values() {
        assert_eq!(LinearTolerance::DEFAULT.value(), 1e-9);
        assert_eq!(AngularTolerance::DEFAULT.value(), 1e-9);
        assert_eq!(ParametricTolerance::DEFAULT.value(), 1e-12);
    }

    #[test]
    #[should_panic(expected = "tolerance must be finite and non-negative")]
    fn negative_tolerance_is_rejected() {
        let _ = LinearTolerance::new(-1.0);
    }
}
