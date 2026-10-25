//! Circles tangent to other curves (#273): of a given radius to two of
//! them — CIRCLE Tan, Tan, Radius — or to three — Tan, Tan, Tan, the
//! problem of Apollonius. A line counts as the infinite line through it,
//! an arc as its whole circle, as the classic commands take them.
//!
//! There are usually several answers; [`nearest_to_picks`] chooses the one
//! a drafter means, whose touching points are nearest where the curves
//! were picked.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::intersect::{circle_circle, line_circle, line_line};
use crate::{Circle, Primitive, Segment};

/// A curve as a tangent sees it: an infinite line, or a full circle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Line(Segment),
    Circle(Circle),
}

impl Curve {
    /// The curve a primitive lies on.
    pub fn of(primitive: &Primitive) -> Self {
        match primitive {
            Primitive::Segment(s) => Self::Line(*s),
            Primitive::Circle(c) => Self::Circle(*c),
            Primitive::Arc(a) => Self::Circle(a.circle()),
        }
    }

    /// Where `circle` touches this curve: a line's point nearest its
    /// centre; a circle's point on the line of centres — on the side
    /// towards `circle`, unless `circle` encloses it, when it is the far
    /// side. `None` for concentric circles.
    pub fn touching_point(&self, circle: &Circle) -> Option<Point2> {
        match self {
            Self::Line(s) => {
                let d = s.end - s.start;
                let length = d.length();
                if length == 0.0 {
                    return None;
                }
                let unit = d * (1.0 / length);
                Some(s.start + unit * (circle.center - s.start).dot(unit))
            }
            Self::Circle(c) => {
                let towards = circle.center - c.center;
                let distance = towards.length();
                if distance == 0.0 {
                    return None;
                }
                let encloses = (distance - (circle.radius - c.radius)).abs()
                    < (distance - (c.radius + circle.radius))
                        .abs()
                        .min((distance - (c.radius - circle.radius)).abs());
                let side = if encloses { -c.radius } else { c.radius };
                Some(c.center + towards * (side / distance))
            }
        }
    }

    /// How far `point` is from this curve.
    pub fn distance_to(&self, point: Point2) -> f64 {
        match self {
            Self::Line(s) => {
                let d = s.end - s.start;
                let length = d.length().max(f64::MIN_POSITIVE);
                (d.x * (point.y - s.start.y) - d.y * (point.x - s.start.x)).abs() / length
            }
            Self::Circle(c) => (c.center.distance_to(point) - c.radius).abs(),
        }
    }

    /// How far `circle` is from touching this curve: zero when tangent.
    pub fn tangency_error(&self, circle: &Circle) -> f64 {
        match self {
            Self::Line(_) => (self.distance_to(circle.center) - circle.radius).abs(),
            Self::Circle(c) => {
                let apart = c.center.distance_to(circle.center);
                [
                    apart - (c.radius + circle.radius),
                    apart - (c.radius - circle.radius),
                    apart - (circle.radius - c.radius),
                ]
                .into_iter()
                .map(f64::abs)
                .fold(f64::INFINITY, f64::min)
            }
        }
    }

    /// The curves a circle of `radius` tangent to this one has its centre
    /// on: two parallel lines, or two concentric circles (one, when the
    /// radii are equal).
    fn offsets(&self, radius: f64) -> Vec<Curve> {
        match self {
            Self::Line(s) => {
                let d = s.end - s.start;
                let length = d.length();
                if length == 0.0 {
                    return Vec::new();
                }
                let normal = cad_core::Vec2::new(-d.y, d.x) * (radius / length);
                [normal, -normal]
                    .into_iter()
                    .map(|n| Self::Line(Segment::new(s.start + n, s.end + n)))
                    .collect()
            }
            Self::Circle(c) => {
                let mut offsets = vec![Self::Circle(Circle::new(c.center, c.radius + radius))];
                let inner = (c.radius - radius).abs();
                if inner > 0.0 {
                    offsets.push(Self::Circle(Circle::new(c.center, inner)));
                }
                offsets
            }
        }
    }
}

/// Where two curves meet.
fn meet(a: &Curve, b: &Curve) -> Vec<Point2> {
    match (a, b) {
        (Curve::Line(p), Curve::Line(q)) => line_line(p, q).into_iter().collect(),
        (Curve::Line(l), Curve::Circle(c)) | (Curve::Circle(c), Curve::Line(l)) => {
            line_circle(l, c).as_slice().to_vec()
        }
        (Curve::Circle(p), Curve::Circle(q)) => circle_circle(p, q).as_slice().to_vec(),
    }
}

/// The scale of the problem: the largest coordinate or radius in it.
fn magnitude(curves: &[&Curve]) -> f64 {
    curves
        .iter()
        .map(|curve| match curve {
            Curve::Line(s) => s
                .start
                .x
                .abs()
                .max(s.start.y.abs())
                .max(s.end.x.abs())
                .max(s.end.y.abs()),
            Curve::Circle(c) => c.center.x.abs().max(c.center.y.abs()) + c.radius,
        })
        .fold(1.0, f64::max)
}

fn distinct(found: &mut Vec<Circle>, circle: Circle, tolerance: f64) {
    let same = |c: &Circle| {
        c.center.distance_to(circle.center) <= tolerance
            && (c.radius - circle.radius).abs() <= tolerance
    };
    if !found.iter().any(same) {
        found.push(circle);
    }
}

/// Every circle of `radius` tangent to both curves: its centre where an
/// offset of one meets an offset of the other. None when they are too far
/// apart, or the radius is not positive.
pub fn tangent_to_two(a: &Curve, b: &Curve, radius: f64) -> Vec<Circle> {
    if !(radius.is_finite() && radius > 0.0) {
        return Vec::new();
    }
    let tolerance = LinearTolerance::DEFAULT
        .at_magnitude(magnitude(&[a, b]) + radius)
        .value();
    let mut found = Vec::new();
    for p in a.offsets(radius) {
        for q in b.offsets(radius) {
            for center in meet(&p, &q) {
                let circle = Circle::new(center, radius);
                // A centre on both offsets touches both curves; checked, as
                // an offset of a circle can coincide with the other curve.
                if a.tangency_error(&circle) <= tolerance * 1e3
                    && b.tangency_error(&circle) <= tolerance * 1e3
                {
                    distinct(&mut found, circle, tolerance * 1e3);
                }
            }
        }
    }
    found
}

/// When `a` and `b` are parallel lines `2·radius` apart, every circle of
/// `radius` between them touches both, and [`tangent_to_two`] gives none:
/// this is the one whose centre is level with `near`.
pub fn between_parallels(a: &Curve, b: &Curve, radius: f64, near: Point2) -> Option<Circle> {
    let (Curve::Line(p), Curve::Line(q)) = (a, b) else {
        return None;
    };
    let d = p.end - p.start;
    let length = d.length();
    if length == 0.0 || !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    let signed = |x: Point2| (d.x * (x.y - p.start.y) - d.y * (x.x - p.start.x)) / length;
    let tolerance = LinearTolerance::DEFAULT
        .at_magnitude(magnitude(&[a, b]) + radius)
        .value()
        * 1e3;
    let (from, to) = (signed(q.start), signed(q.end));
    if (from - to).abs() > tolerance || (from.abs() - 2.0 * radius).abs() > tolerance {
        return None;
    }
    let unit = d * (1.0 / length);
    let normal = cad_core::Vec2::new(-unit.y, unit.x) * (radius * from.signum());
    let along = unit * (near - p.start).dot(unit);
    Some(Circle::new(p.start + along + normal, radius))
}

/// The signed residual of tangency to `curve` in one of its senses: a
/// line's side (±1), or a circle's way of touching (0 outside, 1 inside
/// it, 2 around it).
fn residual(curve: &Curve, sense: i32, x: f64, y: f64, r: f64) -> f64 {
    match curve {
        Curve::Line(s) => {
            let d = s.end - s.start;
            let length = d.length().max(f64::MIN_POSITIVE);
            let signed = (d.x * (y - s.start.y) - d.y * (x - s.start.x)) / length;
            f64::from(sense) * signed - r
        }
        Curve::Circle(c) => {
            let apart = (x - c.center.x).hypot(y - c.center.y);
            match sense {
                0 => apart - (c.radius + r),
                1 => apart - (c.radius - r),
                _ => apart - (r - c.radius),
            }
        }
    }
}

fn senses(curve: &Curve) -> &'static [i32] {
    match curve {
        Curve::Line(_) => &[1, -1],
        Curve::Circle(_) => &[0, 1, 2],
    }
}

/// Newton's method on the three residuals from `start`; the circle, when
/// it converges to one of positive radius.
fn newton(
    curves: [&Curve; 3],
    senses: [i32; 3],
    start: [f64; 3],
    tolerance: f64,
) -> Option<Circle> {
    let mut v = start;
    for _ in 0..60 {
        let f = |v: [f64; 3]| -> [f64; 3] {
            std::array::from_fn(|i| residual(curves[i], senses[i], v[0], v[1], v[2]))
        };
        let r = f(v);
        if r.iter().all(|e| e.abs() <= tolerance) {
            return (v[2] > tolerance).then(|| Circle::new(Point2::new(v[0], v[1]), v[2]));
        }
        // The Jacobian by central differences, and Cramer's rule.
        let scale = v.iter().fold(1.0_f64, |m, x| m.max(x.abs()));
        let h = 1e-7 * scale;
        let mut j = [[0.0; 3]; 3];
        for k in 0..3 {
            let (mut up, mut down) = (v, v);
            up[k] += h;
            down[k] -= h;
            let (fu, fd) = (f(up), f(down));
            for i in 0..3 {
                j[i][k] = (fu[i] - fd[i]) / (2.0 * h);
            }
        }
        let det = |m: [[f64; 3]; 3]| {
            m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
        };
        let d = det(j);
        if d.abs() < 1e-14 * scale * scale {
            return None;
        }
        let mut step = [0.0; 3];
        for (k, out) in step.iter_mut().enumerate() {
            let mut m = j;
            for i in 0..3 {
                m[i][k] = -r[i];
            }
            *out = det(m) / d;
        }
        for k in 0..3 {
            v[k] += step[k];
        }
        if !v.iter().all(|x| x.is_finite()) {
            return None;
        }
    }
    None
}

/// Every circle tangent to all three curves that Newton's method finds
/// from starting points spread about them, in each sense of touching each.
pub fn tangent_to_three(a: &Curve, b: &Curve, c: &Curve) -> Vec<Circle> {
    let curves = [a, b, c];
    let size = magnitude(&curves);
    let tolerance = LinearTolerance::DEFAULT.at_magnitude(size).value();
    // Starting points: the curves' own points, and the middles between them.
    let mut anchors: Vec<Point2> = Vec::new();
    for curve in curves {
        match curve {
            Curve::Line(s) => anchors.extend([s.start, s.end]),
            Curve::Circle(k) => anchors.push(k.center),
        }
    }
    let count = anchors.len();
    for i in 0..count {
        for j in i + 1..count {
            let (p, q) = (anchors[i], anchors[j]);
            anchors.push(Point2::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0));
        }
    }
    let mut found = Vec::new();
    for &sa in senses(a) {
        for &sb in senses(b) {
            for &sc in senses(c) {
                for anchor in &anchors {
                    for guess in [0.1, 0.5, 2.0] {
                        let start = [anchor.x, anchor.y, guess * size.max(1.0) * 0.1];
                        if let Some(circle) = newton(curves, [sa, sb, sc], start, tolerance) {
                            distinct(&mut found, circle, tolerance * 1e3);
                        }
                    }
                }
            }
        }
    }
    found
}

/// Of `candidates`, the circle whose touching points are nearest the
/// picks — `picks[i]` on `curves[i]` — as a drafter means it.
pub fn nearest_to_picks(
    candidates: &[Circle],
    curves: &[Curve],
    picks: &[Point2],
) -> Option<Circle> {
    candidates
        .iter()
        .copied()
        .map(|circle| {
            let cost: f64 = curves
                .iter()
                .zip(picks)
                .map(|(curve, pick)| {
                    curve
                        .touching_point(&circle)
                        .map_or(f64::INFINITY, |p| p.distance_to(*pick))
                })
                .sum();
            (cost, circle)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, circle)| circle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Curve {
        Curve::Line(Segment::new(Point2::new(x0, y0), Point2::new(x1, y1)))
    }

    fn circle(x: f64, y: f64, r: f64) -> Curve {
        Curve::Circle(Circle::new(Point2::new(x, y), r))
    }

    fn tangent_to_all(found: &[Circle], curves: &[Curve]) {
        assert!(!found.is_empty());
        for c in found {
            for curve in curves {
                assert!(curve.tangency_error(c) < 1e-6, "{c:?} {curve:?}");
            }
        }
    }

    #[test]
    fn between_parallels_the_circle_is_where_it_is_asked_for() {
        let (a, b) = (line(0.0, 0.0, 100.0, 0.0), line(100.0, 20.0, 0.0, 20.0));
        assert!(tangent_to_two(&a, &b, 10.0).is_empty());
        let circle = between_parallels(&a, &b, 10.0, Point2::new(30.0, 7.0)).unwrap();
        assert_eq!(circle, Circle::new(Point2::new(30.0, 10.0), 10.0));
        tangent_to_all(&[circle], &[a, b]);
        // Below, too far apart, or not parallel: none.
        let below = line(0.0, -20.0, 100.0, -20.0);
        let circle = between_parallels(&a, &below, 10.0, Point2::new(5.0, 0.0)).unwrap();
        assert_eq!(circle, Circle::new(Point2::new(5.0, -10.0), 10.0));
        assert!(between_parallels(&a, &b, 9.0, Point2::ORIGIN).is_none());
        assert!(
            between_parallels(&a, &line(0.0, 20.0, 100.0, 21.0), 10.0, Point2::ORIGIN).is_none()
        );
    }

    #[test]
    fn two_lines_at_a_right_angle_take_four_circles_of_a_radius() {
        let (a, b) = (line(0.0, 0.0, 10.0, 0.0), line(0.0, 0.0, 0.0, 10.0));
        let found = tangent_to_two(&a, &b, 5.0);
        assert_eq!(found.len(), 4);
        tangent_to_all(&found, &[a, b]);
        let chosen = nearest_to_picks(
            &found,
            &[a, b],
            &[Point2::new(4.0, 0.0), Point2::new(0.0, 6.0)],
        )
        .unwrap();
        assert!(chosen.center.distance_to(Point2::new(5.0, 5.0)) < 1e-9);
    }

    #[test]
    fn a_line_and_a_circle_and_two_circles() {
        let (a, b) = (line(-50.0, 0.0, 50.0, 0.0), circle(0.0, 16.0, 10.0));
        let found = tangent_to_two(&a, &b, 5.0);
        tangent_to_all(&found, &[a, b]);
        let (p, q) = (circle(0.0, 0.0, 10.0), circle(30.0, 0.0, 10.0));
        let found = tangent_to_two(&p, &q, 10.0);
        tangent_to_all(&found, &[p, q]);
        // Too far apart for so small a circle.
        assert!(tangent_to_two(&p, &circle(100.0, 0.0, 10.0), 5.0).is_empty());
    }

    #[test]
    fn three_lines_take_the_incircle_and_excircles() {
        let curves = [
            line(0.0, 0.0, 30.0, 0.0),
            line(30.0, 0.0, 0.0, 40.0),
            line(0.0, 40.0, 0.0, 0.0),
        ];
        let found = tangent_to_three(&curves[0], &curves[1], &curves[2]);
        tangent_to_all(&found, &curves);
        // The 3-4-5 triangle's incircle: radius 10 about (10, 10).
        let inner = nearest_to_picks(
            &found,
            &curves,
            &[
                Point2::new(10.0, 0.0),
                Point2::new(18.0, 16.0),
                Point2::new(0.0, 10.0),
            ],
        )
        .unwrap();
        assert!(
            inner.center.distance_to(Point2::new(10.0, 10.0)) < 1e-7,
            "{inner:?}"
        );
        assert!((inner.radius - 10.0).abs() < 1e-7);
        assert_eq!(found.len(), 4, "{found:?}");
    }

    #[test]
    fn three_circles_and_a_mix() {
        let curves = [
            circle(0.0, 0.0, 10.0),
            circle(40.0, 0.0, 10.0),
            circle(20.0, 30.0, 10.0),
        ];
        let found = tangent_to_three(&curves[0], &curves[1], &curves[2]);
        tangent_to_all(&found, &curves);
        assert!(found.len() >= 2, "{found:?}");
        let mixed = [
            line(-50.0, 0.0, 50.0, 0.0),
            circle(0.0, 30.0, 10.0),
            line(30.0, -50.0, 30.0, 50.0),
        ];
        let found = tangent_to_three(&mixed[0], &mixed[1], &mixed[2]);
        tangent_to_all(&found, &mixed);
    }
}
