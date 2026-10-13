//! Splines (#182): a non-uniform rational B-spline by its degree, control
//! points, knots, and weights, evaluated by de Boor's algorithm; a spline
//! through fit points, interpolated the standard way (Piegl & Tiller 9.2.1:
//! chord-length parameters, knots by averaging, a banded system); and the
//! curve as chords, subdivided until each chord's sagitta is under a
//! tolerance that follows the curve's size, so that what reads the chords —
//! snapping, picking, intersection, rendering — never depends on the view.

use cad_core::{Aabb, Point2, Vec2};

use crate::segment::Segment;
use crate::transform::Transform;

/// The sagitta allowed to a chord, as a fraction of the curve's extent.
pub const CHORD_TOLERANCE: f64 = 2.5e-4;
/// Control points a spline may have, and fit points: a bound on the
/// interpolation's work and on what a file can make the program chew.
pub const MAX_POINTS: usize = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Spline {
    pub degree: usize,
    pub control: Vec<Point2>,
    /// `control.len() + degree + 1` knots, non-decreasing.
    pub knots: Vec<f64>,
    /// One per control point, positive; empty for a non-rational spline.
    pub weights: Vec<f64>,
}

impl Spline {
    /// Whether the geometry is one: a degree of at least one, more
    /// control points than the degree, the right number of knots, in
    /// order, finite, and weights positive when there are any.
    pub fn is_valid(&self) -> bool {
        let n = self.control.len();
        self.degree >= 1
            && n > self.degree
            && n <= MAX_POINTS
            && self.knots.len() == n + self.degree + 1
            && self.knots.iter().all(|knot| knot.is_finite())
            && self.knots.windows(2).all(|pair| pair[0] <= pair[1])
            && self.knots[self.degree] < self.knots[n]
            && self.control.iter().all(|point| point.is_finite())
            && (self.weights.is_empty()
                || (self.weights.len() == n
                    && self
                        .weights
                        .iter()
                        .all(|weight| weight.is_finite() && *weight > 0.0)))
    }

    /// The parameter range the curve is defined over.
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.control.len()])
    }

    /// The knot span `parameter` lies in: the last span at the end.
    fn span(&self, parameter: f64) -> usize {
        let n = self.control.len();
        if parameter >= self.knots[n] {
            return n - 1;
        }
        let (mut low, mut high) = (self.degree, n);
        while high - low > 1 {
            let mid = (low + high) / 2;
            if parameter < self.knots[mid] {
                high = mid;
            } else {
                low = mid;
            }
        }
        low
    }

    /// The point at `parameter`, clamped to the domain: de Boor's algorithm
    /// in homogeneous coordinates when the spline is rational.
    pub fn point_at(&self, parameter: f64) -> Point2 {
        let (start, end) = self.domain();
        let t = parameter.clamp(start, end);
        let p = self.degree;
        let k = self.span(t);
        let weight = |i: usize| self.weights.get(i).copied().unwrap_or(1.0);
        // Homogeneous: (w·x, w·y, w).
        let mut d: Vec<[f64; 3]> = (0..=p)
            .map(|j| {
                let i = k - p + j;
                let w = weight(i);
                [self.control[i].x * w, self.control[i].y * w, w]
            })
            .collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = k - p + j;
                let denominator = self.knots[i + p - r + 1] - self.knots[i];
                let alpha = if denominator > 0.0 {
                    (t - self.knots[i]) / denominator
                } else {
                    0.0
                };
                let previous = d[j - 1];
                for (current, before) in d[j].iter_mut().zip(previous) {
                    *current = (1.0 - alpha) * before + alpha * *current;
                }
            }
        }
        let [x, y, w] = d[p];
        if w.abs() > 0.0 {
            Point2::new(x / w, y / w)
        } else {
            self.control[k]
        }
    }

    pub fn start_point(&self) -> Point2 {
        self.point_at(self.domain().0)
    }

    pub fn end_point(&self) -> Point2 {
        self.point_at(self.domain().1)
    }

    /// A cubic spline through `points`, in order: the parameters by chord
    /// length, the knots by averaging, the control points by solving the
    /// banded interpolation system. `None` for fewer than two points, two
    /// in a row the same, or too many.
    pub fn through(points: &[Point2]) -> Option<Self> {
        let n = points.len();
        if !(2..=MAX_POINTS).contains(&n) || points.iter().any(|point| !point.is_finite()) {
            return None;
        }
        if points.windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        // Fewer points than a cubic needs: a lower degree through them all.
        let degree = (n - 1).min(3);
        let parameters = fit_parameters(points);
        // Knots by averaging: clamped at both ends.
        let mut knots = vec![0.0; n + degree + 1];
        for knot in knots.iter_mut().skip(n) {
            *knot = 1.0;
        }
        for j in 1..n - degree {
            knots[j + degree] = parameters[j..j + degree].iter().sum::<f64>() / degree as f64;
        }
        let mut spline = Self {
            degree,
            control: points.to_vec(),
            knots,
            weights: Vec::new(),
        };
        // The system N·P = Q, N banded with bandwidth `degree`.
        let rows: Vec<Vec<f64>> = parameters.iter().map(|t| spline.basis_row(*t)).collect();
        let control = solve_banded(&rows, points, degree)?;
        spline.control = control;
        spline.is_valid().then_some(spline)
    }

    /// The basis functions at `t`: one weight per control point.
    fn basis_row(&self, t: f64) -> Vec<f64> {
        let n = self.control.len();
        let p = self.degree;
        let (start, end) = self.domain();
        let t = t.clamp(start, end);
        let k = self.span(t);
        let mut row = vec![0.0; n];
        // Cox–de Boor on the p + 1 non-zero functions.
        let mut basis = vec![0.0; p + 1];
        basis[0] = 1.0;
        let mut left = vec![0.0; p + 1];
        let mut right = vec![0.0; p + 1];
        for j in 1..=p {
            left[j] = t - self.knots[k + 1 - j];
            right[j] = self.knots[k + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                let denominator = right[r + 1] + left[j - r];
                let temp = if denominator > 0.0 {
                    basis[r] / denominator
                } else {
                    0.0
                };
                basis[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            basis[j] = saved;
        }
        for (j, value) in basis.iter().enumerate() {
            row[k - p + j] = *value;
        }
        row
    }

    /// The curve as points along it, from start to end, with every chord's
    /// sagitta under [`CHORD_TOLERANCE`] of the control polygon's extent.
    pub fn chord_points(&self) -> Vec<Point2> {
        let extent = self.control.iter().fold(
            Aabb::from_points(self.control[0], self.control[0]),
            |b, p| b.union(&Aabb::from_points(*p, *p)),
        );
        let size = extent.min().distance_to(extent.max());
        let tolerance = (size * CHORD_TOLERANCE).max(1e-12);
        let (start, end) = self.domain();
        // Start from the knot spans, so no feature between two samples is
        // skipped, then subdivide each until flat.
        let mut parameters: Vec<f64> = self
            .knots
            .iter()
            .copied()
            .filter(|knot| *knot > start && *knot < end)
            .collect();
        parameters.dedup();
        let mut out = vec![self.point_at(start)];
        let mut from = start;
        for to in parameters.into_iter().chain(std::iter::once(end)) {
            self.flatten_between(from, to, tolerance, 0, &mut out);
            from = to;
        }
        out
    }

    /// Splits `[a, b]` until the curve lies within `tolerance` of each
    /// chord at the quarter points as well as the middle — a midpoint alone
    /// misses an S-shaped span — and at least three levels deep, so that
    /// a span of the knots is never a single chord.
    fn flatten_between(&self, a: f64, b: f64, tolerance: f64, depth: u32, out: &mut Vec<Point2>) {
        let (pa, pb) = (self.point_at(a), self.point_at(b));
        let mid = (a + b) / 2.0;
        let chord = Segment::new(pa, pb);
        let off = |t: f64| {
            let point = self.point_at(t);
            if pa == pb {
                point.distance_to(pa)
            } else {
                chord.distance_to_point(point)
            }
        };
        let flat = depth >= 3
            && [0.25, 0.5, 0.75]
                .iter()
                .all(|f| off(a + (b - a) * f) <= tolerance);
        if flat || depth >= 16 {
            out.push(pb);
        } else {
            self.flatten_between(a, mid, tolerance, depth + 1, out);
            self.flatten_between(mid, b, tolerance, depth + 1, out);
        }
    }

    /// The curve as chords.
    pub fn segments(&self) -> Vec<Segment> {
        self.chord_points()
            .windows(2)
            .map(|pair| Segment::new(pair[0], pair[1]))
            .collect()
    }

    /// The box the chords fit in — the curve's, to the chord tolerance.
    pub fn bounds(&self) -> Aabb {
        let points = self.chord_points();
        points
            .iter()
            .skip(1)
            .fold(Aabb::from_points(points[0], points[0]), |b, p| {
                b.union(&Aabb::from_points(*p, *p))
            })
    }

    /// The nearest point of the chords to `point`.
    pub fn closest_point(&self, point: Point2) -> Point2 {
        self.segments()
            .iter()
            .map(|segment| segment.closest_point(point))
            .min_by(|a, b| a.distance_to(point).total_cmp(&b.distance_to(point)))
            .unwrap_or(self.control[0])
    }

    pub fn distance_to_point(&self, point: Point2) -> f64 {
        self.closest_point(point).distance_to(point)
    }

    /// The image under a similarity: the control points move, the rest
    /// stays. A B-spline is affinely invariant, so the curve moves with them.
    pub fn transformed(&self, transform: &Transform) -> Self {
        Self {
            control: self
                .control
                .iter()
                .map(|point| transform.apply(*point))
                .collect(),
            ..self.clone()
        }
    }
}

/// The parameter of each fit point, by chord length, from 0 to 1.
pub fn fit_parameters(points: &[Point2]) -> Vec<f64> {
    let n = points.len();
    let mut parameters = vec![0.0; n];
    let total: f64 = points
        .windows(2)
        .map(|pair| pair[0].distance_to(pair[1]))
        .sum();
    for i in 1..n {
        parameters[i] = parameters[i - 1] + points[i - 1].distance_to(points[i]) / total;
    }
    if n > 1 {
        parameters[n - 1] = 1.0;
    }
    parameters
}

/// Solves `rows · x = rhs` for points, the rows banded about the diagonal
/// with bandwidth `band`, by Gaussian elimination without pivoting (the
/// system is totally positive). `None` when a pivot vanishes.
// The index runs over two rows of the matrix at once: a loop by index is
// the plain way to write it.
#[allow(clippy::needless_range_loop)]
fn solve_banded(rows: &[Vec<f64>], rhs: &[Point2], band: usize) -> Option<Vec<Point2>> {
    let n = rows.len();
    let mut a: Vec<Vec<f64>> = rows.to_vec();
    let mut b: Vec<Vec2> = rhs.iter().map(|p| *p - Point2::ORIGIN).collect();
    for k in 0..n {
        let pivot = a[k][k];
        if pivot.abs() < 1e-300 {
            return None;
        }
        for i in k + 1..(k + band + 1).min(n) {
            let factor = a[i][k] / pivot;
            if factor == 0.0 {
                continue;
            }
            for j in k..(k + band + 1).min(n) {
                a[i][j] -= factor * a[k][j];
            }
            b[i] = b[i] - b[k] * factor;
        }
    }
    let mut x = vec![Vec2::new(0.0, 0.0); n];
    for k in (0..n).rev() {
        let mut sum = b[k];
        for j in k + 1..(k + band + 1).min(n) {
            sum = sum - x[j] * a[k][j];
        }
        x[k] = sum * (1.0 / a[k][k]);
    }
    Some(x.into_iter().map(|v| Point2::ORIGIN + v).collect())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn a_spline_through_points_passes_through_them_in_order() {
        let points = [p(0.0, 0.0), p(10.0, 10.0), p(20.0, 0.0), p(30.0, 5.0)];
        let spline = Spline::through(&points).unwrap();
        assert_eq!(spline.degree, 3);
        assert_eq!(spline.control.len(), 4);
        assert!(spline.is_valid());
        assert!(spline.start_point().distance_to(points[0]) < 1e-9);
        assert!(spline.end_point().distance_to(points[3]) < 1e-9);
        for (point, t) in points.iter().zip(fit_parameters(&points)) {
            assert!(spline.point_at(t).distance_to(*point) < 1e-9);
            let off = spline.distance_to_point(*point);
            assert!(off < 0.02, "{point:?} is {off} from the chords");
        }
        // Two points make a straight line of degree one.
        let line = Spline::through(&[p(0.0, 0.0), p(10.0, 0.0)]).unwrap();
        assert_eq!(line.degree, 1);
        assert!(line.point_at(0.5).distance_to(p(5.0, 0.0)) < 1e-12);
        assert!(Spline::through(&[p(0.0, 0.0)]).is_none());
        assert!(Spline::through(&[p(0.0, 0.0), p(0.0, 0.0), p(1.0, 1.0)]).is_none());
    }

    #[test]
    fn a_circle_as_a_rational_quadratic_is_round() {
        // The classic nine-control-point circle of radius 1.
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let control = vec![
            p(1.0, 0.0),
            p(1.0, 1.0),
            p(0.0, 1.0),
            p(-1.0, 1.0),
            p(-1.0, 0.0),
            p(-1.0, -1.0),
            p(0.0, -1.0),
            p(1.0, -1.0),
            p(1.0, 0.0),
        ];
        let weights = vec![1.0, s, 1.0, s, 1.0, s, 1.0, s, 1.0];
        let knots = vec![
            0.0, 0.0, 0.0, 0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0, 1.0,
        ];
        let circle = Spline {
            degree: 2,
            control,
            knots,
            weights,
        };
        assert!(circle.is_valid());
        for i in 0..=40 {
            let point = circle.point_at(i as f64 / 40.0);
            assert!(
                (point.distance_to(Point2::ORIGIN) - 1.0).abs() < 1e-12,
                "{point:?}"
            );
        }
        for chord in circle.segments() {
            let mid = chord.start + (chord.end - chord.start) * 0.5;
            assert!(1.0 - mid.distance_to(Point2::ORIGIN) < 2.0 * CHORD_TOLERANCE * 2.0);
        }
    }

    #[test]
    fn what_is_not_a_spline_is_refused() {
        let good = Spline::through(&[p(0.0, 0.0), p(1.0, 2.0), p(3.0, 1.0)]).unwrap();
        let mut bad = good.clone();
        bad.knots.pop();
        assert!(!bad.is_valid());
        let mut bad = good.clone();
        bad.knots.swap(2, 3);
        bad.knots[3] = -1.0;
        assert!(!bad.is_valid());
        let mut bad = good.clone();
        bad.weights = vec![1.0, 0.0, 1.0];
        assert!(!bad.is_valid());
        let mut bad = good;
        bad.degree = 0;
        assert!(!bad.is_valid());
    }

    fn fit_points() -> impl Strategy<Value = Vec<Point2>> {
        prop::collection::vec((-100.0f64..100.0, -100.0f64..100.0), 2..12).prop_map(|pairs| {
            let mut points: Vec<Point2> = pairs.into_iter().map(|(x, y)| p(x, y)).collect();
            points.dedup();
            points
        })
    }

    proptest! {
        #[test]
        fn chords_hug_the_curve_and_the_curve_passes_through_its_fit_points(points in fit_points()) {
            prop_assume!(points.len() >= 2);
            let spline = Spline::through(&points).unwrap();
            let extent = spline.bounds();
            let size = extent.min().distance_to(extent.max()).max(1e-9);
            // Every fit point is on the curve, at its parameter.
            for (point, t) in points.iter().zip(fit_parameters(&points)) {
                let off = spline.point_at(t).distance_to(*point);
                prop_assert!(off < 1e-9 * (1.0 + size), "{point:?} is {off} off the curve");
            }
            // Every seventh chord's midpoint is near the curve.
            for segment in spline.segments().into_iter().step_by(7) {
                let mid = segment.start + (segment.end - segment.start) * 0.5;
                let distance = spline.closest_point(mid).distance_to(mid);
                prop_assert!(distance <= 1e-9 + size * CHORD_TOLERANCE * 2.0, "{distance} vs {size}");
            }
        }

        #[test]
        fn a_similarity_moves_the_curve_with_its_control_points(points in fit_points(), degrees in -180.0f64..180.0, factor in 0.2f64..5.0, f in 0.0f64..=1.0) {
            prop_assume!(points.len() >= 2);
            let spline = Spline::through(&points).unwrap();
            let (start, end) = spline.domain();
            let point = spline.point_at(start + (end - start) * f);
            for transform in [
                Transform::translation(Vec2::new(3.0, -4.0)),
                Transform::rotation(p(1.0, 2.0), degrees),
                Transform::scaling(p(1.0, 2.0), factor).unwrap(),
                Transform::mirror(p(0.0, 0.0), p(1.0, 1.0)).unwrap(),
            ] {
                let image = spline.transformed(&transform);
                let expected = transform.apply(point);
                let got = image.point_at(start + (end - start) * f);
                prop_assert!(got.distance_to(expected) < 1e-9 * (1.0 + expected.x.abs() + expected.y.abs()));
            }
        }
    }
}
