//! Isometric drafting (#336, ADR-0040): an isometric *drawing* on three
//! axes at true length — `x` along 30°, `y` along 150°, `z` up, the eye
//! looking along (−1, −1, 1), above the nearest corner — and the three
//! planes they pair into. Needed by the drafting aids, the renderer's
//! grid, ELLIPSE, and the intent compiler alike.

use crate::{Point2, Vec2};

/// cos 30°: how far across an isometric axis runs per unit along it.
const COS_30: f64 = 0.866_025_403_784_438_6;

/// An isometric axis's unit vector on the page.
pub const X: Vec2 = Vec2::new(COS_30, 0.5);
pub const Y: Vec2 = Vec2::new(-COS_30, 0.5);
pub const Z: Vec2 = Vec2::new(0.0, 1.0);

/// Where the isometric point `(x, y, z)` falls on the page, the origin at
/// the page's origin.
pub fn map(x: f64, y: f64, z: f64) -> Point2 {
    Point2::ORIGIN + X * x + Y * y + Z * z
}

/// A face of an isometric box: the plane two axes span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Plane {
    /// `y` and `z`.
    Left,
    /// `x` and `y`.
    #[default]
    Top,
    /// `x` and `z`.
    Right,
}

impl Plane {
    pub const ALL: [Self; 3] = [Self::Left, Self::Top, Self::Right];

    /// The two axes the plane holds, on the page.
    pub fn axes(self) -> [Vec2; 2] {
        match self {
            Self::Left => [Y, Z],
            Self::Top => [X, Y],
            Self::Right => [X, Z],
        }
    }

    /// The plane after this one, as F5 cycles them: left, top, right.
    pub fn next(self) -> Self {
        match self {
            Self::Left => Self::Top,
            Self::Top => Self::Right,
            Self::Right => Self::Left,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Top => "Top",
            Self::Right => "Right",
        }
    }

    /// The ellipse a circle of `radius` on the plane is drawn as: its
    /// major half-axis on the page, and the minor over the major. The
    /// circle's points `r (a cos t + b sin t)`, for the plane's axes `a`
    /// and `b` at 60° or 120°, reach furthest along `a + b` or `a − b`,
    /// whichever is longer: `r √(3/2)` one way, `r / √2` the other.
    pub fn circle(self, radius: f64) -> (Vec2, f64) {
        let [a, b] = self.axes();
        let (sum, difference) = (a + b, a - b);
        let (major, minor) = if sum.length() >= difference.length() {
            (sum, difference)
        } else {
            (difference, sum)
        };
        let half = radius / std::f64::consts::SQRT_2;
        (major * half, minor.length() / major.length())
    }
}

/// The nearest point of the isometric snap lattice to `point`: the points
/// `i·s·X + j·s·Y`, whose rows run along 30° and 150° and whose columns
/// are vertical, `s` apart up a column. A spacing that is not a positive
/// number leaves the point free.
pub fn lattice(point: Point2, spacing: f64) -> Point2 {
    if !(spacing.is_finite() && spacing > 0.0) {
        return point;
    }
    // point = i·s·X + j·s·Y: x = (i − j)·s·cos 30°, y = (i + j)·s / 2.
    let across = point.x / (spacing * COS_30);
    let up = 2.0 * point.y / spacing;
    let (i, j) = ((across + up) / 2.0, (up - across) / 2.0);
    // The rhombus of the lattice holding the point is two equilateral
    // triangles, each nearest one of its corners.
    let corners = [
        (i.floor(), j.floor()),
        (i.floor(), j.ceil()),
        (i.ceil(), j.floor()),
        (i.ceil(), j.ceil()),
    ];
    corners
        .into_iter()
        .map(|(i, j)| Point2::ORIGIN + (X * i + Y * j) * spacing)
        .min_by(|a, b| a.distance_to(point).total_cmp(&b.distance_to(point)))
        .expect("four corners")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_axes_are_at_30_150_and_90_degrees_and_a_unit_long() {
        for (axis, degrees) in [(X, 30.0_f64), (Y, 150.0), (Z, 90.0)] {
            assert!(close(axis.length(), 1.0));
            assert!(close(axis.y.atan2(axis.x).to_degrees(), degrees));
        }
        // A unit cube's far top corner is two units up the page.
        let corner = map(1.0, 1.0, 1.0);
        assert!(close(corner.x, 0.0) && close(corner.y, 2.0));
    }

    /// The map is an orthographic isometric projection, scaled to true
    /// length (an isometric drawing, √(3/2) times the projection): its
    /// two rows — where x, y, and z go across and up — are square to each
    /// other and equally long, so every axis is foreshortened alike; and
    /// the eye looks along (−1, −1, 1), from above the corner nearest it,
    /// the lowest on the page. The positive axes leave that corner at 30°,
    /// 150°, and 90°; as lines the three axes are 60° apart, and the rays
    /// along x, along y, and down z are 120° apart. Drawing the axes at
    /// 330°, 210°, and 90° instead is the same projection seen from the
    /// opposite octant, (1, 1, 1): the nearest corner the highest.
    #[test]
    fn the_map_is_an_isometric_projection_seen_from_minus_x_minus_y_above() {
        let across = [X.x, Y.x, Z.x];
        let up = [X.y, Y.y, Z.y];
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        assert!(close(dot(across, up), 0.0));
        assert!(close(dot(across, across), 1.5) && close(dot(up, up), 1.5));
        // The eye: across × up, towards the viewer.
        let eye = [
            across[1] * up[2] - across[2] * up[1],
            across[2] * up[0] - across[0] * up[2],
            across[0] * up[1] - across[1] * up[0],
        ];
        let k = eye[2];
        assert!(k > 0.0 && close(eye[0], -k) && close(eye[1], -k));
        // The rays along x, along y, and down z, 120° apart.
        let angle = |v: Vec2| v.y.atan2(v.x).to_degrees().rem_euclid(360.0);
        let rays = [angle(X), angle(Y), angle(Z * -1.0)];
        assert!(close(rays[1] - rays[0], 120.0) && close(rays[2] - rays[1], 120.0));
    }

    #[test]
    fn an_isocircle_is_the_circle_on_its_plane_and_touches_its_square() {
        for plane in Plane::ALL {
            let (major, ratio) = plane.circle(10.0);
            assert!(close(major.length(), 10.0 * 1.5_f64.sqrt()), "{plane:?}");
            assert!(close(ratio, 1.0 / 3.0_f64.sqrt()), "{plane:?}");
            let minor = Vec2::new(-major.y, major.x) * ratio;
            let [a, b] = plane.axes();
            // Every point of the circle on the plane is on the ellipse:
            // its coordinates on the ellipse's axes satisfy u² + v² = 1.
            for k in 0..36 {
                let t = f64::from(k) * 10_f64.to_radians();
                let q = (a * t.cos() + b * t.sin()) * 10.0;
                let u = q.dot(major) / major.length_squared();
                let v = q.dot(minor) / minor.length_squared();
                assert!(close(u * u + v * v, 1.0), "{plane:?} at {k}");
            }
            // It touches the middle of each side of its square: ±a and ±b
            // are on it.
            for q in [a * 10.0, b * 10.0] {
                let u = q.dot(major) / major.length_squared();
                let v = q.dot(minor) / minor.length_squared();
                assert!(close(u * u + v * v, 1.0));
            }
        }
        // The top plane's long axis is level; the left's at 120°, the right's at 60°.
        let angle = |plane: Plane| {
            let (major, _) = plane.circle(1.0);
            major.y.atan2(major.x).to_degrees().rem_euclid(180.0)
        };
        assert!(close(angle(Plane::Top), 0.0) || close(angle(Plane::Top), 180.0));
        assert!(close(angle(Plane::Left), 120.0));
        assert!(close(angle(Plane::Right), 60.0));
    }

    #[test]
    fn the_lattice_takes_the_nearest_of_its_points() {
        let s = 10.0;
        // Lattice points stay put; the vertical through one holds one every s.
        for (i, j) in [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (3.0, -2.0),
            (-4.0, -4.0),
        ] {
            let p = Point2::ORIGIN + (X * i + Y * j) * s;
            assert!(lattice(p, s).distance_to(p) < 1e-9);
        }
        assert!(lattice(Point2::new(0.0, 31.0), s).distance_to(Point2::new(0.0, 30.0)) < 1e-9);
        // A point near one goes to it; brute force agrees anywhere.
        for k in 0..400 {
            let p = Point2::new(
                f64::from(k % 20) * 3.7 - 37.0,
                f64::from(k / 20) * 2.9 - 29.0,
            );
            let snapped = lattice(p, s);
            let best = (-10..=10)
                .flat_map(|i| (-10..=10).map(move |j| (i, j)))
                .map(|(i, j)| Point2::ORIGIN + (X * f64::from(i) + Y * f64::from(j)) * s)
                .map(|q| q.distance_to(p))
                .fold(f64::INFINITY, f64::min);
            assert!(close(snapped.distance_to(p), best), "{p:?}");
        }
        // No spacing, no snap.
        let p = Point2::new(1.3, 2.7);
        assert_eq!(lattice(p, 0.0), p);
    }

    #[test]
    fn f5_cycles_the_planes() {
        assert_eq!(Plane::Left.next(), Plane::Top);
        assert_eq!(Plane::Top.next(), Plane::Right);
        assert_eq!(Plane::Right.next(), Plane::Left);
        assert_eq!(Plane::default(), Plane::Top);
    }
}
