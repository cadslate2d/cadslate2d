//! Points and vectors in world space. Document geometry is `f64` (DESIGN §21).

use std::ops::{Add, Mul, Neg, Sub};

/// A position in the world coordinate system.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

/// A displacement between two points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub const ORIGIN: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(self, other: Self) -> f64 {
        (other - self).length()
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// A vector of `length` at `angle` radians, counter-clockwise from +X.
    pub fn from_polar(length: f64, angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::new(length * cos, length * sin)
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
}

impl Sub for Point2 {
    type Output = Vec2;
    fn sub(self, rhs: Self) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Add<Vec2> for Point2 {
    type Output = Point2;
    fn add(self, rhs: Vec2) -> Point2 {
        Point2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub<Vec2> for Point2 {
    type Output = Point2;
    fn sub(self, rhs: Vec2) -> Point2 {
        Point2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Self) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Self) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, rhs: f64) -> Vec2 {
        Vec2::new(self.x * rhs, self.y * rhs)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_difference_is_a_vector() {
        assert_eq!(
            Point2::new(5.0, 7.0) - Point2::new(2.0, 3.0),
            Vec2::new(3.0, 4.0)
        );
    }

    #[test]
    fn point_plus_vector_round_trips() {
        let p = Point2::new(1.5, -2.0);
        let v = Vec2::new(10.0, 4.0);
        assert_eq!((p + v) - v, p);
    }

    #[test]
    fn distance_is_euclidean() {
        assert_eq!(Point2::ORIGIN.distance_to(Point2::new(3.0, 4.0)), 5.0);
    }

    #[test]
    fn polar_vector_points_counter_clockwise_from_x() {
        let v = Vec2::from_polar(2.0, std::f64::consts::FRAC_PI_2);
        assert!(v.x.abs() < 1e-15);
        assert!((v.y - 2.0).abs() < 1e-15);
    }

    #[test]
    fn vector_arithmetic() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, -1.0);
        assert_eq!(a + b, Vec2::new(4.0, 1.0));
        assert_eq!(a - b, Vec2::new(-2.0, 3.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));
        assert_eq!(a.dot(b), 1.0);
        assert_eq!(a.length_squared(), 5.0);
    }

    #[test]
    fn non_finite_points_are_detected() {
        assert!(Point2::new(1.0, 2.0).is_finite());
        assert!(!Point2::new(f64::NAN, 2.0).is_finite());
        assert!(!Point2::new(1.0, f64::INFINITY).is_finite());
    }
}
