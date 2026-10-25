//! Geometric and dimensional constraints, solved numerically (ADR-0038).
//!
//! A [`System`] is a vector of unknowns — the coordinates of points and the
//! radii of circles the constraints refer to — and scalar [`Equation`]s over
//! them, each zero when its constraint holds. Some unknowns are *held*: the
//! ones an edit has just set, which the solver must not move.
//!
//! [`solve`] splits the system into independent components (unknowns joined
//! by an equation), leaves alone every component already satisfied, and
//! settles each of the others by damped Gauss–Newton (Levenberg–Marquardt)
//! from where the geometry is: every step is the minimum-norm change, in the
//! least-squares sense, that reduces the residuals of the linearised
//! equations — the nearest solution exactly when the equations are linear,
//! a near one when they are not. The
//! linear system of each step is solved by conjugate gradients over the
//! sparse Jacobian, which is what keeps a thousand constraints interactive.
//!
//! A component that cannot be satisfied is reported with the equations left
//! unsatisfied; nothing is half-applied. [`assess`] tells whether a new
//! constraint fixes something the others leave free, adds nothing
//! (redundant), or contradicts them (conflicting) — the over-constraint a
//! constraint command refuses — and [`freedom`] counts the degrees of
//! freedom left: both by a sparse elimination of the Jacobian, at a generic
//! configuration near the current one so that a degenerate one (two points
//! that happen to meet) is not taken for a redundant constraint.
//!
//! The result never depends on hashing: components are kept and pivots
//! chosen in ordered maps, and perturbations come from a fixed seed.

use std::collections::BTreeMap;

use cad_core::tolerance::{AngularTolerance, LinearTolerance};

/// An index into [`System::values`].
pub type Param = usize;

/// A point of the system: its two unknowns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointVar {
    pub x: Param,
    pub y: Param,
}

/// A line of the system: its two ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineVar {
    pub a: PointVar,
    pub b: PointVar,
}

/// A circle or an arc's circle: its centre and radius.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircleVar {
    pub center: PointVar,
    pub radius: Param,
}

/// How two circles touch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tangency {
    /// Each outside the other.
    Outside,
    /// The first inside the second.
    FirstInside,
    /// The second inside the first.
    SecondInside,
}

/// One scalar equation, zero when it holds. A constraint is one or more of
/// them: coincidence is two, `Equal` of each coordinate.
#[derive(Clone, Debug, PartialEq)]
pub enum Equation {
    /// An unknown at a value: a fixed coordinate, a radius dimension.
    Fixed { param: Param, value: f64 },
    /// Two unknowns equal: one coordinate of a coincidence, a horizontal
    /// line's two `y`s, two equal radii.
    Equal { a: Param, b: Param },
    /// `to − from = value`: a horizontal or vertical distance.
    Offset { from: Param, to: Param, value: f64 },
    /// The distance between two points.
    Distance {
        a: PointVar,
        b: PointVar,
        value: f64,
    },
    /// A point on the infinite line through a line's ends.
    PointOnLine { point: PointVar, line: LineVar },
    /// A point on a circle.
    PointOnCircle { point: PointVar, circle: CircleVar },
    /// Two lines parallel (the sine of the angle between them).
    Parallel { a: LineVar, b: LineVar },
    /// Two lines perpendicular (the cosine of the angle between them).
    Perpendicular { a: LineVar, b: LineVar },
    /// The angle from one line to another, radians, counter-clockwise.
    Angle { a: LineVar, b: LineVar, value: f64 },
    /// Two lines of one length.
    EqualLength { a: LineVar, b: LineVar },
    /// A line touching a circle, on the side of it the circle's centre is
    /// (`side` +1 left of the line's direction, −1 right).
    LineTangent {
        line: LineVar,
        circle: CircleVar,
        side: f64,
    },
    /// Two circles touching.
    CircleTangent {
        a: CircleVar,
        b: CircleVar,
        tangency: Tangency,
    },
    /// The midpoint of two points on a line: one half of symmetry.
    MidpointOnLine {
        a: PointVar,
        b: PointVar,
        line: LineVar,
    },
    /// Two points on a perpendicular to a line: the other half.
    PerpendicularTo {
        a: PointVar,
        b: PointVar,
        line: LineVar,
    },
}

/// What an equation's residual measures, and so the tolerance it is held to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Length,
    Angle,
}

fn at(values: &[f64], point: PointVar) -> (f64, f64) {
    (values[point.x], values[point.y])
}

fn direction(values: &[f64], line: LineVar) -> (f64, f64) {
    let (ax, ay) = at(values, line.a);
    let (bx, by) = at(values, line.b);
    (bx - ax, by - ay)
}

/// A length that is never zero, for dividing by: a line whose ends meet has
/// no direction, and its equations are as unsatisfied as can be told.
fn length(dx: f64, dy: f64) -> f64 {
    dx.hypot(dy).max(f64::MIN_POSITIVE.sqrt())
}

/// The signed distance of a point from a line, positive on its left.
fn signed_distance(values: &[f64], point: PointVar, line: LineVar) -> f64 {
    let (dx, dy) = direction(values, line);
    let (ax, ay) = at(values, line.a);
    let (px, py) = at(values, point);
    (dx * (py - ay) - dy * (px - ax)) / length(dx, dy)
}

fn wrap(angle: f64) -> f64 {
    let turned = angle.rem_euclid(std::f64::consts::TAU);
    if turned > std::f64::consts::PI {
        turned - std::f64::consts::TAU
    } else {
        turned
    }
}

impl Equation {
    /// The residual at `values`: zero when the equation holds.
    pub fn residual(&self, values: &[f64]) -> f64 {
        match *self {
            Self::Fixed { param, value } => values[param] - value,
            Self::Equal { a, b } => values[a] - values[b],
            Self::Offset { from, to, value } => values[to] - values[from] - value,
            Self::Distance { a, b, value } => {
                let (ax, ay) = at(values, a);
                let (bx, by) = at(values, b);
                (bx - ax).hypot(by - ay) - value
            }
            Self::PointOnLine { point, line } => signed_distance(values, point, line),
            Self::PointOnCircle { point, circle } => {
                let (px, py) = at(values, point);
                let (cx, cy) = at(values, circle.center);
                (px - cx).hypot(py - cy) - values[circle.radius]
            }
            Self::Parallel { a, b } => {
                let (ax, ay) = direction(values, a);
                let (bx, by) = direction(values, b);
                (ax * by - ay * bx) / (length(ax, ay) * length(bx, by))
            }
            Self::Perpendicular { a, b } => {
                let (ax, ay) = direction(values, a);
                let (bx, by) = direction(values, b);
                (ax * bx + ay * by) / (length(ax, ay) * length(bx, by))
            }
            Self::Angle { a, b, value } => {
                let (ax, ay) = direction(values, a);
                let (bx, by) = direction(values, b);
                wrap((ax * by - ay * bx).atan2(ax * bx + ay * by) - value)
            }
            Self::EqualLength { a, b } => {
                let (ax, ay) = direction(values, a);
                let (bx, by) = direction(values, b);
                ax.hypot(ay) - bx.hypot(by)
            }
            Self::LineTangent { line, circle, side } => {
                side * signed_distance(values, circle.center, line) - values[circle.radius]
            }
            Self::CircleTangent { a, b, tangency } => {
                let (ax, ay) = at(values, a.center);
                let (bx, by) = at(values, b.center);
                let apart = (bx - ax).hypot(by - ay);
                let (ra, rb) = (values[a.radius], values[b.radius]);
                match tangency {
                    Tangency::Outside => apart - (ra + rb),
                    Tangency::FirstInside => apart - (rb - ra),
                    Tangency::SecondInside => apart - (ra - rb),
                }
            }
            Self::MidpointOnLine { a, b, line } => {
                let (ax, ay) = at(values, a);
                let (bx, by) = at(values, b);
                let (dx, dy) = direction(values, line);
                let (lx, ly) = at(values, line.a);
                let (mx, my) = (0.5 * (ax + bx), 0.5 * (ay + by));
                (dx * (my - ly) - dy * (mx - lx)) / length(dx, dy)
            }
            Self::PerpendicularTo { a, b, line } => {
                let (ax, ay) = at(values, a);
                let (bx, by) = at(values, b);
                let (dx, dy) = direction(values, line);
                ((bx - ax) * dx + (by - ay) * dy) / length(dx, dy)
            }
        }
    }

    /// The unknowns the residual depends on.
    pub fn params(&self) -> Vec<Param> {
        let point = |p: PointVar| [p.x, p.y];
        let line = |l: LineVar| [l.a.x, l.a.y, l.b.x, l.b.y];
        let circle = |c: CircleVar| [c.center.x, c.center.y, c.radius];
        match *self {
            Self::Fixed { param, .. } => vec![param],
            Self::Equal { a, b } => vec![a, b],
            Self::Offset { from, to, .. } => vec![from, to],
            Self::Distance { a, b, .. } => [point(a), point(b)].concat(),
            Self::PointOnLine { point: p, line: l } => [&point(p)[..], &line(l)].concat(),
            Self::PointOnCircle {
                point: p,
                circle: c,
            } => [&point(p)[..], &circle(c)].concat(),
            Self::Parallel { a, b }
            | Self::Perpendicular { a, b }
            | Self::Angle { a, b, .. }
            | Self::EqualLength { a, b } => [line(a), line(b)].concat(),
            Self::LineTangent {
                line: l, circle: c, ..
            } => [&line(l)[..], &circle(c)].concat(),
            Self::CircleTangent { a, b, .. } => [circle(a), circle(b)].concat(),
            Self::MidpointOnLine { a, b, line: l } | Self::PerpendicularTo { a, b, line: l } => {
                [&point(a)[..], &point(b), &line(l)].concat()
            }
        }
    }

    pub fn measure(&self) -> Measure {
        match self {
            Self::Parallel { .. } | Self::Perpendicular { .. } | Self::Angle { .. } => {
                Measure::Angle
            }
            _ => Measure::Length,
        }
    }
}

/// Unknowns, which of them an edit holds, and the equations over them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct System {
    pub values: Vec<f64>,
    /// Unknowns the solver must not move; as long as `values`, or empty
    /// when none is held.
    pub held: Vec<bool>,
    pub equations: Vec<Equation>,
}

impl System {
    /// A new unknown at `value`.
    pub fn param(&mut self, value: f64) -> Param {
        self.values.push(value);
        if !self.held.is_empty() {
            self.held.push(false);
        }
        self.values.len() - 1
    }

    pub fn point(&mut self, x: f64, y: f64) -> PointVar {
        PointVar {
            x: self.param(x),
            y: self.param(y),
        }
    }

    /// Holds `param` where it is.
    pub fn hold(&mut self, param: Param) {
        if self.held.is_empty() {
            self.held = vec![false; self.values.len()];
        }
        self.held[param] = true;
    }

    fn is_held(&self, param: Param) -> bool {
        self.held.get(param).copied().unwrap_or(false)
    }
}

/// The tolerances a solution is held to: lengths, at the magnitude of the
/// coordinates (ADR-0012), and angles.
#[derive(Clone, Copy, Debug)]
pub struct Tolerances {
    pub linear: LinearTolerance,
    pub angular: AngularTolerance,
}

impl Default for Tolerances {
    fn default() -> Self {
        Self {
            linear: LinearTolerance::DEFAULT,
            angular: AngularTolerance::DEFAULT,
        }
    }
}

/// What solving came to.
#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    /// The settled unknowns; the system's own where it could not be solved.
    pub values: Vec<f64>,
    /// The equations left unsatisfied, by index: empty when solved.
    pub unsatisfied: Vec<usize>,
    /// Gauss–Newton steps taken, over every component.
    pub iterations: usize,
}

impl Solution {
    pub fn is_solved(&self) -> bool {
        self.unsatisfied.is_empty()
    }
}

/// The most steps a component is given.
const MAX_STEPS: usize = 100;

/// Union–find over the unknowns.
struct Components {
    parent: Vec<usize>,
}

impl Components {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn root(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    fn join(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.root(a), self.root(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// The equations of `system` in independent groups, each with its free
/// unknowns; equations with no free unknown are a group of their own with
/// none.
fn components(system: &System) -> Vec<(Vec<Param>, Vec<usize>)> {
    let mut sets = Components::new(system.values.len());
    for equation in &system.equations {
        let free: Vec<Param> = equation
            .params()
            .into_iter()
            .filter(|&p| !system.is_held(p))
            .collect();
        for pair in free.windows(2) {
            sets.join(pair[0], pair[1]);
        }
    }
    let mut groups: BTreeMap<Option<usize>, (Vec<Param>, Vec<usize>)> = BTreeMap::new();
    for (index, equation) in system.equations.iter().enumerate() {
        let first = equation.params().into_iter().find(|&p| !system.is_held(p));
        let key = first.map(|p| sets.root(p));
        groups.entry(key).or_default().1.push(index);
    }
    for param in 0..system.values.len() {
        if !system.is_held(param) {
            let key = Some(sets.root(param));
            if let Some(group) = groups.get_mut(&key) {
                group.0.push(param);
            }
        }
    }
    groups.into_values().collect()
}

/// The scale lengths in a group are measured against: the extent of its
/// coordinates, for the tolerance at their magnitude and for weighing an
/// angle's residual against a length's.
fn magnitude(values: &[f64], params: &[Param]) -> f64 {
    params.iter().map(|&p| values[p].abs()).fold(1.0, f64::max)
}

/// Whether equation `index` holds at `values`.
fn holds(equation: &Equation, values: &[f64], linear: f64, angular: f64) -> bool {
    let residual = equation.residual(values).abs();
    match equation.measure() {
        Measure::Length => residual <= linear,
        Measure::Angle => residual <= angular,
    }
}

/// The equations of a group, the free unknowns it moves, and the scale.
struct Group<'a> {
    equations: Vec<&'a Equation>,
    /// Local column of each free unknown.
    columns: BTreeMap<Param, usize>,
    free: Vec<Param>,
    /// What an angle's residual is multiplied by: a length.
    scale: f64,
}

impl Group<'_> {
    /// The residuals, angles weighed as lengths.
    fn residuals(&self, values: &[f64]) -> Vec<f64> {
        self.equations
            .iter()
            .map(|e| {
                let r = e.residual(values);
                if e.measure() == Measure::Angle {
                    r * self.scale
                } else {
                    r
                }
            })
            .collect()
    }

    /// The Jacobian, a sparse row per equation, by central differences.
    fn jacobian(&self, values: &mut [f64]) -> Vec<Vec<(usize, f64)>> {
        self.equations
            .iter()
            .map(|e| {
                let weight = if e.measure() == Measure::Angle {
                    self.scale
                } else {
                    1.0
                };
                let mut row = Vec::new();
                let mut seen = Vec::new();
                for param in e.params() {
                    let Some(&column) = self.columns.get(&param) else {
                        continue;
                    };
                    if seen.contains(&param) {
                        continue;
                    }
                    seen.push(param);
                    let x = values[param];
                    // About the cube root of the machine epsilon, relative:
                    // the step that balances truncation against rounding.
                    let h = 6e-6 * x.abs().max(self.scale);
                    values[param] = x + h;
                    let up = e.residual(values);
                    values[param] = x - h;
                    let down = e.residual(values);
                    values[param] = x;
                    let slope = weight * (up - down) / (2.0 * h);
                    if slope != 0.0 {
                        row.push((column, slope));
                    }
                }
                row
            })
            .collect()
    }
}

/// `(Jᵀ J + μ I) v` without forming `Jᵀ J`.
fn normal_times(rows: &[Vec<(usize, f64)>], mu: f64, v: &[f64]) -> Vec<f64> {
    let mut out: Vec<f64> = v.iter().map(|x| mu * x).collect();
    for row in rows {
        let dot: f64 = row.iter().map(|&(c, a)| a * v[c]).sum();
        for &(c, a) in row {
            out[c] += a * dot;
        }
    }
    out
}

/// Solves `(Jᵀ J + μ I) x = b` by conjugate gradients, preconditioned by
/// the diagonal.
fn conjugate_gradients(rows: &[Vec<(usize, f64)>], mu: f64, b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut diagonal = vec![mu; n];
    for row in rows {
        for &(c, a) in row {
            diagonal[c] += a * a;
        }
    }
    let precondition = |r: &[f64]| -> Vec<f64> {
        r.iter()
            .zip(&diagonal)
            .map(|(x, d)| if *d > 0.0 { x / d } else { *x })
            .collect()
    };
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let mut x = vec![0.0; n];
    let mut r = b.to_vec();
    let mut z = precondition(&r);
    let mut p = z.clone();
    let mut rz = dot(&r, &z);
    let target = 1e-28 * dot(b, b);
    for _ in 0..(2 * n).max(10) {
        if dot(&r, &r) <= target {
            break;
        }
        let ap = normal_times(rows, mu, &p);
        let pap = dot(&p, &ap);
        if pap <= 0.0 {
            break;
        }
        let alpha = rz / pap;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        z = precondition(&r);
        let next = dot(&r, &z);
        let beta = next / rz;
        rz = next;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
    }
    x
}

/// Settles one group in place; the steps it took, and whether it holds.
///
/// Levenberg–Marquardt on the weighted residuals `r` — an angle's residual
/// multiplied by the group's extent `L`, so `W` is 1 for lengths and `L²`
/// for angles — from the current values:
///
/// - each step solves `(Jᵀ W J + μ I) Δ = −Jᵀ W r`, symmetric and positive
///   definite since `μ > 0`, by conjugate gradients preconditioned by its
///   diagonal, from `Δ = 0`, until the residual of that system is 10⁻¹⁴ of
///   its right-hand side or after `2n` iterations; `μ = λ · max diag(Jᵀ W J)`;
/// - a step is taken when it lowers `rᵀ W r`, and `λ` falls fourfold (to no
///   less than 10⁻¹⁵); otherwise `λ` grows eightfold and the step is tried
///   again;
/// - it stops when every equation holds to its tolerance (solved), when `λ`
///   passes 10¹⁰, or after [`MAX_STEPS`] steps (not solved: the caller
///   keeps the values it started from).
///
/// `D = I` makes every step's displacement lie in the row space of `J`:
/// for constraints linear in the unknowns the result is the solution
/// nearest the start, the orthogonal projection onto them. For non-linear
/// ones it is a solution reached by small steps from the start — near it,
/// not provably the nearest.
fn settle(group: &Group<'_>, values: &mut [f64], linear: f64, angular: f64) -> (usize, bool) {
    let satisfied = |values: &[f64]| {
        group
            .equations
            .iter()
            .all(|e| holds(e, values, linear, angular))
    };
    let cost = |r: &[f64]| r.iter().map(|x| x * x).sum::<f64>();
    let mut lambda = 1e-6;
    let mut residuals = group.residuals(values);
    for step in 0..MAX_STEPS {
        if satisfied(values) {
            return (step, true);
        }
        let rows = group.jacobian(values);
        let mut gradient = vec![0.0; group.free.len()];
        let mut largest: f64 = 0.0;
        for (row, r) in rows.iter().zip(&residuals) {
            for &(c, a) in row {
                gradient[c] -= a * r;
                largest = largest.max(a * a);
            }
        }
        let before = cost(&residuals);
        loop {
            let mu = lambda * largest.max(f64::MIN_POSITIVE);
            let delta = conjugate_gradients(&rows, mu, &gradient);
            let mut trial = values.to_vec();
            for (&param, d) in group.free.iter().zip(&delta) {
                trial[param] += d;
            }
            let after = group.residuals(&trial);
            if cost(&after) < before {
                values.copy_from_slice(&trial);
                residuals = after;
                lambda = (lambda / 4.0).max(1e-15);
                break;
            }
            lambda *= 8.0;
            if lambda > 1e10 {
                return (step, satisfied(values));
            }
        }
    }
    (MAX_STEPS, satisfied(values))
}

/// Solves `system`: every component that does not hold is settled from its
/// current values by small least-squares steps ([`settle`]), its held
/// unknowns unmoved. If any component cannot be settled, every value is
/// returned as it was — nothing is half-applied — with the equations that
/// component left unsatisfied at its least-squares compromise, the ones to
/// blame. Components are independent, so the order they are found in
/// changes nothing.
pub fn solve(system: &System, tolerances: Tolerances) -> Solution {
    let mut values = system.values.clone();
    let mut unsatisfied = Vec::new();
    let mut iterations = 0;
    for (free, indices) in components(system) {
        let scale = magnitude(&system.values, &free);
        let linear = tolerances.linear.at_magnitude(scale).value();
        let angular = tolerances.angular.value();
        if indices
            .iter()
            .all(|&i| holds(&system.equations[i], &values, linear, angular))
        {
            continue;
        }
        if free.is_empty() {
            unsatisfied.extend(indices);
            continue;
        }
        let group = Group {
            equations: indices.iter().map(|&i| &system.equations[i]).collect(),
            columns: free.iter().enumerate().map(|(c, &p)| (p, c)).collect(),
            free: free.clone(),
            scale,
        };
        let mut settled = values.clone();
        let (steps, solved) = settle(&group, &mut settled, linear, angular);
        iterations += steps;
        if solved {
            for &param in &free {
                values[param] = settled[param];
            }
        } else {
            unsatisfied.extend(
                indices
                    .iter()
                    .copied()
                    .filter(|&i| !holds(&system.equations[i], &settled, linear, angular)),
            );
        }
    }
    unsatisfied.sort_unstable();
    // Nothing is half-applied: a held unknown can part one constraint
    // graph into components, and one settling while another cannot would
    // leave geometry half-moved.
    if !unsatisfied.is_empty() {
        values.clone_from(&system.values);
    }
    Solution {
        values,
        unsatisfied,
        iterations,
    }
}

/// The Jacobian's rows reduced to echelon form, sparse: each pivot row has
/// a 1 in its pivot column and no entry in the pivot column of any row
/// before it. Reducing a row against them tells whether it is a
/// combination of theirs; the pivots counted are the Jacobian's rank.
/// Ordered maps throughout, so the pivots chosen never depend on hashing.
struct Echelon {
    pivots: Vec<(usize, Vec<(usize, f64)>)>,
    /// Entries smaller than this are zero: a fraction of the largest.
    threshold: f64,
}

impl Echelon {
    fn new(threshold: f64) -> Self {
        Self {
            pivots: Vec::new(),
            threshold,
        }
    }

    /// `row` less its combination of the pivot rows.
    fn reduce(&self, row: &[(usize, f64)]) -> BTreeMap<usize, f64> {
        let mut left: BTreeMap<usize, f64> = BTreeMap::new();
        for &(c, a) in row {
            *left.entry(c).or_insert(0.0) += a;
        }
        for (column, pivot) in &self.pivots {
            let Some(&factor) = left.get(column) else {
                continue;
            };
            if factor == 0.0 {
                continue;
            }
            for &(c, a) in pivot {
                *left.entry(c).or_insert(0.0) -= factor * a;
            }
            left.remove(column);
        }
        left.retain(|_, a| a.abs() > self.threshold);
        left
    }

    /// Adds `row`; whether it raised the rank.
    fn push(&mut self, row: &[(usize, f64)]) -> bool {
        let left = self.reduce(row);
        // The largest entry, the lowest column among equals.
        let Some((&column, &lead)) = left
            .iter()
            .rev()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        else {
            return false;
        };
        let pivot: Vec<(usize, f64)> = left.into_iter().map(|(c, a)| (c, a / lead)).collect();
        self.pivots.push((column, pivot));
        true
    }
}

/// The rank of the Jacobian of `equations` over the free unknowns of
/// `system`, at `values`.
fn rank(system: &System, equations: &[&Equation], values: &[f64]) -> usize {
    let free: Vec<Param> = (0..system.values.len())
        .filter(|&p| !system.is_held(p))
        .collect();
    let group = Group {
        equations: equations.to_vec(),
        columns: free.iter().enumerate().map(|(c, &p)| (p, c)).collect(),
        free: free.clone(),
        scale: magnitude(values, &free),
    };
    let mut at = values.to_vec();
    let rows = group.jacobian(&mut at);
    let largest = rows
        .iter()
        .flatten()
        .fold(0.0_f64, |m, &(_, a)| m.max(a.abs()));
    let mut echelon = Echelon::new(1e-9 * largest.max(f64::MIN_POSITIVE));
    rows.iter().filter(|row| echelon.push(row)).count()
}

/// SplitMix64: the same small numbers on every platform, for perturbing.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A generic configuration of `system` near its own: its free unknowns
/// moved by small, fixed pseudo-random amounts and settled back onto its
/// equations. Rank there is the constraints' rank, not an accident of
/// where the geometry happens to be — two ends of a line that happen to
/// meet give it no direction, though nothing makes them meet. Its own
/// values when it cannot be settled from there.
fn witness(system: &System, tolerances: Tolerances) -> Vec<f64> {
    let free: Vec<Param> = (0..system.values.len())
        .filter(|&p| !system.is_held(p))
        .collect();
    let scale = magnitude(&system.values, &free);
    let mut moved = system.clone();
    let mut state = 0x00C0_FFEE_u64;
    for &param in &free {
        let unit = (splitmix(&mut state) >> 11) as f64 / (1u64 << 53) as f64;
        moved.values[param] += (unit - 0.5) * 2e-3 * scale;
    }
    let settled = solve(&moved, tolerances);
    if settled.is_solved() {
        settled.values
    } else {
        system.values.clone()
    }
}

/// What a constraint would do to a system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It fixes what the others leave free: each of its equations raises
    /// the rank. Solving then says whether it can be met.
    Independent,
    /// It adds nothing the others do not fix, and already holds: it over-
    /// constrains the geometry, harmlessly, and is refused.
    Redundant,
    /// It adds nothing the others do not fix, and does not hold: it
    /// contradicts them, and is refused.
    Conflicting,
}

/// A constraint's verdict, and whether the geometry is where its equations
/// lose rank though the constraints do not — a zero-length line, two
/// circles that happen to share a centre — so that what the current
/// configuration alone says would mislead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assessment {
    pub verdict: Verdict,
    pub degenerate: bool,
}

/// What adding a whole constraint — every equation of it, together —
/// would do to `system`. Rank is judged at a generic configuration
/// ([`witness`]) near the current one, among the components the
/// constraint touches only, so that a coincidence one of whose two
/// equations is already implied is over-constraining, and a degenerate
/// configuration is told apart from a redundant constraint.
pub fn assess(system: &System, constraint: &[Equation], tolerances: Tolerances) -> Assessment {
    let existing = system.equations.len();
    let mut with = system.clone();
    with.equations.extend(constraint.iter().cloned());
    // Only the components the new equations join can hold a combination
    // of them.
    let mut touched: Vec<usize> = components(&with)
        .into_iter()
        .filter(|(_, indices)| indices.iter().any(|&i| i >= existing))
        .flat_map(|(_, indices)| indices)
        .collect();
    touched.sort_unstable();
    let old: Vec<&Equation> = touched
        .iter()
        .filter(|&&i| i < existing)
        .map(|&i| &with.equations[i])
        .collect();
    let all: Vec<&Equation> = touched.iter().map(|&i| &with.equations[i]).collect();
    let mut before = system.clone();
    before.equations = old.iter().map(|&e| e.clone()).collect();
    let generic = witness(&before, tolerances);
    let rise = |values: &[f64]| rank(&with, &all, values) - rank(&with, &old, values);
    let at_generic = rise(&generic);
    let here = rise(&system.values);
    let verdict = if at_generic == constraint.len() {
        Verdict::Independent
    } else {
        let scale = magnitude(
            &system.values,
            &(0..system.values.len()).collect::<Vec<_>>(),
        );
        let linear = tolerances.linear.at_magnitude(scale).value();
        let angular = tolerances.angular.value();
        if constraint
            .iter()
            .all(|e| holds(e, &system.values, linear, angular))
        {
            Verdict::Redundant
        } else {
            Verdict::Conflicting
        }
    };
    Assessment {
        verdict,
        degenerate: here < at_generic,
    }
}

/// Degrees of freedom left in `system`: its free unknowns less the rank of
/// its equations, at a generic configuration near its own.
pub fn freedom(system: &System, tolerances: Tolerances) -> usize {
    let free = (0..system.values.len())
        .filter(|&p| !system.is_held(p))
        .count();
    let equations: Vec<&Equation> = system.equations.iter().collect();
    free - rank(system, &equations, &witness(system, tolerances))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rectangle of four lines, its corners coincident, two sides
    /// horizontal and two vertical: four points shared, so 8 unknowns.
    #[derive(Clone, Debug)]
    struct Rectangle {
        corners: [PointVar; 4],
    }

    fn sides(r: &Rectangle) -> [LineVar; 4] {
        let c = r.corners;
        [
            LineVar { a: c[0], b: c[1] },
            LineVar { a: c[1], b: c[2] },
            LineVar { a: c[2], b: c[3] },
            LineVar { a: c[3], b: c[0] },
        ]
    }

    fn rectangle(system: &mut System, x: f64, y: f64, w: f64, h: f64) -> Rectangle {
        let corners = [
            system.point(x, y),
            system.point(x + w, y),
            system.point(x + w, y + h),
            system.point(x, y + h),
        ];
        let r = Rectangle { corners };
        let s = sides(&r);
        system.equations.extend([
            Equation::Equal {
                a: s[0].a.y,
                b: s[0].b.y,
            },
            Equation::Equal {
                a: s[2].a.y,
                b: s[2].b.y,
            },
            Equation::Equal {
                a: s[1].a.x,
                b: s[1].b.x,
            },
            Equation::Equal {
                a: s[3].a.x,
                b: s[3].b.x,
            },
        ]);
        r
    }

    #[test]
    fn a_satisfied_system_is_left_as_it_is() {
        let mut system = System::default();
        let r = rectangle(&mut system, 0.0, 0.0, 120.0, 80.0);
        system.equations.push(Equation::Distance {
            a: r.corners[0],
            b: r.corners[1],
            value: 120.0,
        });
        let solution = solve(&system, Tolerances::default());
        assert!(solution.is_solved());
        assert_eq!(solution.values, system.values);
        assert_eq!(solution.iterations, 0);
    }

    #[test]
    fn a_dimension_changed_moves_the_least_it_must() {
        let mut system = System::default();
        let r = rectangle(&mut system, 0.0, 0.0, 120.0, 80.0);
        // The lower left corner fixed; the width now 140.
        system.equations.extend([
            Equation::Fixed {
                param: r.corners[0].x,
                value: 0.0,
            },
            Equation::Fixed {
                param: r.corners[0].y,
                value: 0.0,
            },
            Equation::Offset {
                from: r.corners[0].x,
                to: r.corners[1].x,
                value: 140.0,
            },
            Equation::Offset {
                from: r.corners[0].y,
                to: r.corners[3].y,
                value: 80.0,
            },
        ]);
        let solution = solve(&system, Tolerances::default());
        assert!(solution.is_solved(), "{solution:?}");
        let v = &solution.values;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-7;
        assert!(near(v[r.corners[1].x], 140.0) && near(v[r.corners[2].x], 140.0));
        assert!(near(v[r.corners[2].y], 80.0) && near(v[r.corners[3].y], 80.0));
        assert!(near(v[r.corners[3].x], 0.0));
        assert_eq!(freedom(&system, Tolerances::default()), 0);
    }

    #[test]
    fn held_unknowns_stay_and_the_rest_follow() {
        let mut system = System::default();
        let r = rectangle(&mut system, 0.0, 0.0, 100.0, 50.0);
        // A grip drags the upper right corner to (130, 70).
        system.values[r.corners[2].x] = 130.0;
        system.values[r.corners[2].y] = 70.0;
        system.hold(r.corners[2].x);
        system.hold(r.corners[2].y);
        let solution = solve(&system, Tolerances::default());
        assert!(solution.is_solved());
        let v = &solution.values;
        assert_eq!((v[r.corners[2].x], v[r.corners[2].y]), (130.0, 70.0));
        assert!((v[r.corners[1].x] - 130.0).abs() < 1e-7);
        assert!((v[r.corners[3].y] - 70.0).abs() < 1e-7);
    }

    /// A grip holds the upper right corner of a fixed, dimensioned plate
    /// 5 higher: the hold parts the plate's equations into components, the
    /// top settles but the rest cannot, and nothing moves at all.
    #[test]
    fn a_held_unknown_that_cannot_be_met_moves_nothing() {
        let mut system = System::default();
        let r = rectangle(&mut system, 0.0, 0.0, 120.0, 80.0);
        system.equations.extend([
            Equation::Fixed {
                param: r.corners[0].x,
                value: 0.0,
            },
            Equation::Fixed {
                param: r.corners[0].y,
                value: 0.0,
            },
            Equation::Offset {
                from: r.corners[1].y,
                to: r.corners[2].y,
                value: 80.0,
            },
        ]);
        system.values[r.corners[2].y] = 85.0;
        system.hold(r.corners[2].y);
        let solution = solve(&system, Tolerances::default());
        assert!(!solution.is_solved());
        assert_eq!(solution.values, system.values);
        assert!(
            solution.unsatisfied.contains(&(system.equations.len() - 1)),
            "{solution:?}"
        );
    }

    #[test]
    fn a_conflict_is_reported_and_nothing_moves() {
        let mut system = System::default();
        let a = system.point(0.0, 0.0);
        let b = system.point(10.0, 0.0);
        let c = system.point(20.0, 0.0);
        system.equations.extend([
            Equation::Distance { a, b, value: 10.0 },
            Equation::Distance {
                a: b,
                b: c,
                value: 10.0,
            },
            Equation::Distance {
                a,
                b: c,
                value: 50.0,
            },
        ]);
        let solution = solve(&system, Tolerances::default());
        assert!(!solution.is_solved());
        assert!(!solution.unsatisfied.is_empty());
        assert_eq!(solution.values, system.values);
    }

    #[test]
    fn tangency_parallels_and_angles_settle() {
        let mut system = System::default();
        let a = system.point(0.0, 0.0);
        let b = system.point(100.0, 3.0);
        let line = LineVar { a, b };
        let center = system.point(50.0, 30.0);
        let radius = system.param(10.0);
        let circle = CircleVar { center, radius };
        let c = system.point(0.0, 40.0);
        let d = system.point(90.0, 45.0);
        let other = LineVar { a: c, b: d };
        system.equations.extend([
            Equation::LineTangent {
                line,
                circle,
                side: 1.0,
            },
            Equation::Parallel { a: line, b: other },
            Equation::Angle {
                a: line,
                b: LineVar { a, b: center },
                value: 0.5,
            },
        ]);
        let solution = solve(&system, Tolerances::default());
        assert!(solution.is_solved(), "{solution:?}");
        for e in &system.equations {
            assert!(e.residual(&solution.values).abs() < 1e-8, "{e:?}");
        }
    }

    fn assessed(system: &System, constraint: &[Equation]) -> Assessment {
        assess(system, constraint, Tolerances::default())
    }

    #[test]
    fn a_constraint_already_implied_is_redundant() {
        let mut system = System::default();
        let r = rectangle(&mut system, 0.0, 0.0, 120.0, 80.0);
        let s = sides(&r);
        // Opposite sides of a rectangle are already parallel.
        let parallel = Equation::Parallel { a: s[0], b: s[2] };
        assert_eq!(assessed(&system, &[parallel]).verdict, Verdict::Redundant);
        // A width is not implied.
        let width = Equation::Offset {
            from: r.corners[0].x,
            to: r.corners[1].x,
            value: 120.0,
        };
        assert_eq!(assessed(&system, &[width]).verdict, Verdict::Independent);
        assert_eq!(freedom(&system, Tolerances::default()), 4);
    }

    #[test]
    fn an_implied_constraint_that_does_not_hold_conflicts() {
        let mut system = System::default();
        let r = rectangle(&mut system, 0.0, 0.0, 120.0, 80.0);
        let s = sides(&r);
        // The bottom and top are already parallel: at 30 degrees they
        // cannot be.
        let angle = Equation::Angle {
            a: s[0],
            b: s[2],
            value: 0.5,
        };
        assert_eq!(assessed(&system, &[angle]).verdict, Verdict::Conflicting);
    }

    /// A constraint is judged whole: a coincidence of two points already
    /// on one horizontal has one of its equations implied, so it over-
    /// constrains, though its other equation would fix something.
    #[test]
    fn a_constraint_is_judged_by_all_its_equations_together() {
        let mut system = System::default();
        let p = system.point(0.0, 5.0);
        let q = system.point(0.0, 5.0);
        system.equations.push(Equation::Equal { a: p.y, b: q.y });
        let coincident = [
            Equation::Equal { a: p.x, b: q.x },
            Equation::Equal { a: p.y, b: q.y },
        ];
        assert_eq!(assessed(&system, &coincident).verdict, Verdict::Redundant);
        assert_eq!(
            assessed(&system, &coincident[..1]).verdict,
            Verdict::Independent
        );
    }

    /// Two points that happen to meet have no direction between them, so a
    /// distance between them has no gradient there; nothing makes them
    /// meet, so the distance is independent, and the configuration
    /// degenerate.
    #[test]
    fn a_degenerate_configuration_is_not_taken_for_redundancy() {
        let mut system = System::default();
        let a = system.point(10.0, 10.0);
        let b = system.point(10.0, 10.0);
        let distance = Equation::Distance { a, b, value: 5.0 };
        let assessment = assessed(&system, &[distance]);
        assert_eq!(assessment.verdict, Verdict::Independent);
        assert!(assessment.degenerate);
        // Two points that are constrained to meet are not degenerate.
        let mut held = System::default();
        let p = held.point(1.0, 1.0);
        let q = held.point(1.0, 1.0);
        held.equations.extend([
            Equation::Equal { a: p.x, b: q.x },
            Equation::Equal { a: p.y, b: q.y },
        ]);
        let again = assessed(&held, &[Equation::Equal { a: p.x, b: q.x }]);
        assert_eq!(again.verdict, Verdict::Redundant);
        assert!(!again.degenerate);
    }

    /// For constraints that are linear in the unknowns, the solution is the
    /// one nearest the start: changing a distance between two free points
    /// moves both by half the change, not one by all of it.
    #[test]
    fn linear_constraints_settle_at_the_nearest_solution() {
        let mut system = System::default();
        let a = system.point(0.0, 0.0);
        let b = system.point(10.0, 0.0);
        system.equations.push(Equation::Offset {
            from: a.x,
            to: b.x,
            value: 14.0,
        });
        let solution = solve(&system, Tolerances::default());
        assert!(solution.is_solved());
        assert!(
            (solution.values[a.x] + 2.0).abs() < 1e-9,
            "{:?}",
            solution.values
        );
        assert!((solution.values[b.x] - 12.0).abs() < 1e-9);
        assert_eq!((solution.values[a.y], solution.values[b.y]), (0.0, 0.0));
    }

    /// Solving is deterministic: the same system gives the same bits, and
    /// the same equations in another order the same geometry within
    /// tolerance.
    #[test]
    fn solving_is_deterministic() {
        let mut system = System::default();
        let r = rectangle(&mut system, 0.0, 0.0, 100.0, 50.0);
        system.equations.extend([
            Equation::Fixed {
                param: r.corners[0].x,
                value: 0.0,
            },
            Equation::Fixed {
                param: r.corners[0].y,
                value: 0.0,
            },
            Equation::Distance {
                a: r.corners[0],
                b: r.corners[2],
                value: 130.0,
            },
            Equation::Angle {
                a: LineVar {
                    a: r.corners[0],
                    b: r.corners[1],
                },
                b: LineVar {
                    a: r.corners[0],
                    b: r.corners[2],
                },
                value: 0.4,
            },
        ]);
        let first = solve(&system, Tolerances::default());
        assert!(first.is_solved());
        assert_eq!(first, solve(&system, Tolerances::default()));
        let mut reversed = system.clone();
        reversed.equations.reverse();
        let other = solve(&reversed, Tolerances::default());
        for (a, b) in first.values.iter().zip(&other.values) {
            assert!((a - b).abs() < 1e-7, "{a} {b}");
        }
        let assessment = assess(
            &system,
            &[Equation::Parallel {
                a: sides(&r)[0],
                b: sides(&r)[2],
            }],
            Tolerances::default(),
        );
        assert_eq!(
            assessment,
            assess(
                &system,
                &[Equation::Parallel {
                    a: sides(&r)[0],
                    b: sides(&r)[2],
                }],
                Tolerances::default()
            )
        );
    }

    use proptest::prelude::*;

    /// Plates as `rectangle` makes them, each with its width and height
    /// dimensioned and its lower-left corner fixed where it is: satisfied
    /// as built.
    fn plates() -> impl Strategy<Value = (System, Vec<Rectangle>)> {
        prop::collection::vec(
            (-1e3..1e3f64, -1e3..1e3f64, 1.0..500.0f64, 1.0..500.0f64),
            1..5,
        )
        .prop_map(|specs| {
            let mut system = System::default();
            let mut plates = Vec::new();
            for (x, y, w, h) in specs {
                let r = rectangle(&mut system, x, y, w, h);
                let v = &system.values;
                let (width, height) = (
                    v[r.corners[1].x] - v[r.corners[0].x],
                    v[r.corners[3].y] - v[r.corners[0].y],
                );
                system.equations.extend([
                    Equation::Fixed {
                        param: r.corners[0].x,
                        value: x,
                    },
                    Equation::Fixed {
                        param: r.corners[0].y,
                        value: y,
                    },
                    Equation::Offset {
                        from: r.corners[0].x,
                        to: r.corners[1].x,
                        value: width,
                    },
                    Equation::Offset {
                        from: r.corners[0].y,
                        to: r.corners[3].y,
                        value: height,
                    },
                ]);
                plates.push(r);
            }
            (system, plates)
        })
    }

    /// The index of the width equation of plate `i`.
    fn width_of(system: &System, plate: &Rectangle) -> usize {
        system
            .equations
            .iter()
            .position(|e| matches!(e, Equation::Offset { from, to, .. } if *from == plate.corners[0].x && *to == plate.corners[1].x))
            .expect("a width")
    }

    fn all_hold(system: &System, values: &[f64]) -> bool {
        let scale = values.iter().fold(1.0_f64, |m, v| m.max(v.abs()));
        let linear = LinearTolerance::DEFAULT.at_magnitude(scale).value();
        system
            .equations
            .iter()
            .all(|e| holds(e, values, linear, AngularTolerance::DEFAULT.value()))
    }

    proptest! {
        /// A system that holds is left exactly as it is: no step taken,
        /// not a bit moved.
        #[test]
        fn satisfied_constraints_stay_satisfied((system, _) in plates()) {
            let solution = solve(&system, Tolerances::default());
            prop_assert!(solution.is_solved());
            prop_assert_eq!(solution.iterations, 0);
            prop_assert_eq!(&solution.values, &system.values);
        }

        /// A width changed, a corner perhaps held where a grip put it: the
        /// solution holds every equation, leaves held unknowns where they
        /// are, and solving it again changes nothing; or, unsolved, it
        /// moves nothing at all.
        #[test]
        fn a_solution_holds_everything_and_is_settled(
            (mut system, plates) in plates(),
            pick in any::<usize>(),
            width in 1.0..600.0f64,
            drag in prop::option::of((-50.0..50.0f64, -50.0..50.0f64)),
        ) {
            let plate = &plates[pick % plates.len()];
            let index = width_of(&system, plate);
            if let Equation::Offset { value, .. } = &mut system.equations[index] {
                *value = width;
            }
            if let Some((dx, dy)) = drag {
                let corner = plate.corners[2];
                system.values[corner.x] += dx;
                system.values[corner.y] += dy;
                system.hold(corner.x);
                system.hold(corner.y);
            }
            let solution = solve(&system, Tolerances::default());
            if solution.is_solved() {
                prop_assert!(all_hold(&system, &solution.values));
                for (param, held) in system.held.iter().enumerate() {
                    if *held {
                        prop_assert_eq!(solution.values[param], system.values[param]);
                    }
                }
                let mut again = system.clone();
                again.values.clone_from(&solution.values);
                prop_assert_eq!(&solve(&again, Tolerances::default()).values, &solution.values);
            } else {
                prop_assert_eq!(&solution.values, &system.values);
                prop_assert!(!solution.unsatisfied.is_empty());
            }
            // Without a drag the plate is fixed at one corner and nothing
            // contradicts the new width: it is always met.
            if drag.is_none() {
                prop_assert!(solution.is_solved());
            }
        }

        /// A second width for a plate that already has one is a conflict:
        /// assessed as one, and, forced in, unsolvable with nothing moved.
        /// The same width again is redundant.
        #[test]
        fn conflicts_are_detected(
            (system, plates) in plates(),
            pick in any::<usize>(),
            off in prop_oneof![-100.0..-0.01f64, 0.01..100.0f64],
        ) {
            let plate = &plates[pick % plates.len()];
            let Equation::Offset { from, to, value } = system.equations[width_of(&system, plate)].clone() else {
                unreachable!()
            };
            let same = Equation::Offset { from, to, value };
            prop_assert_eq!(assess(&system, &[same], Tolerances::default()).verdict, Verdict::Redundant);
            let other = Equation::Offset { from, to, value: value + off };
            prop_assert_eq!(assess(&system, std::slice::from_ref(&other), Tolerances::default()).verdict, Verdict::Conflicting);
            let mut forced = system.clone();
            forced.equations.push(other);
            let solution = solve(&forced, Tolerances::default());
            prop_assert!(!solution.is_solved());
            prop_assert_eq!(&solution.values, &forced.values);
            prop_assert_eq!(freedom(&system, Tolerances::default()), 0);
        }
    }
}
