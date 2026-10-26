//! The search: slicings as normalised Polish expressions, sized by area,
//! judged by the constraints, and improved by simulated annealing.

use cad_core::tolerance::LinearTolerance;

use crate::{CIRCULATION, Layout, PlacedRoom, Rect, Refusal, Spec};

/// The weight of a hard constraint's violation, per metre or per room, in
/// the energy the annealing descends. Low, so the search crosses
/// infeasible plans on its way (measured: 100 found plans half as often);
/// which plan wins is decided lexicographically, never by this weight.
const HARD: f64 = 3.0;
const W_AREA: f64 = 10.0;
const W_RATIO: f64 = 1.0;
const W_CONNECTIVITY: f64 = 0.5;
const W_PREFERENCES: f64 = 2.0;
/// How far a room's area may be from its target.
pub const AREA_TOLERANCE: f64 = 0.05;
/// A door keeps this far from the walls at either end of its wall.
pub const DOOR_CLEARANCE: f64 = 0.15;
/// Steps of the search per room.
const STEPS_PER_ROOM: usize = 1500;
const T_START: f64 = 10.0;
const T_END: f64 = 0.005;
/// How many times the sizes are corrected for the walls.
const SIZING_PASSES: usize = 6;
/// The weight of the circulation space, per its share of the rooms' floor:
/// it is there only when it buys more than it costs.
const W_SPARE: f64 = 5.0;
/// The least floor of a circulation space, when there is one.
const SPARE_LEAST: f64 = 1.5;
/// The search's budget for every plan asked for, in steps: the searches
/// it makes, each from a seed of its own, before a brief that may have a
/// plan is called exhausted. Forty searches of six rooms; fewer of more,
/// so an exhausted search takes about as long whatever the brief's size.
const STEP_BUDGET: usize = 40 * 6 * STEPS_PER_ROOM;
/// The fewest and the most searches for a plan.
const SEARCHES: (usize, usize) = (8, 40);
/// Of them, the share made without a circulation space: one is added only
/// when the rooms alone gave no plan.
const WITHOUT_CIRCULATION: f64 = 0.25;

/// A token of a Polish expression: a room, or a cut of the two
/// rectangles before it — side by side (`V`, a vertical cut) or one above
/// the other (`H`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    Room(usize),
    V,
    H,
}

impl Tok {
    fn is_cut(self) -> bool {
        !matches!(self, Tok::Room(_))
    }
}

/// Where the search is: the slicing, the outline's width, and the
/// circulation space's floor on centrelines (none at zero).
#[derive(Clone, Debug)]
struct State {
    expr: Vec<Tok>,
    width: f64,
    spare: f64,
}

/// A small, fast, seeded generator (SplitMix64): the same seed, the same
/// plan, on every machine.
#[derive(Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// In [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// What a brief asks, worked out once.
#[derive(Debug)]
struct Problem<'a> {
    spec: &'a Spec,
    n: usize,
    buildable: Rect,
    te: f64,
    ti: f64,
    /// The wall a door needs: its width, its clearances, and the walls at
    /// either end.
    door_span: f64,
    entrance: usize,
    pairs: Vec<(usize, usize)>,
    tolerance: LinearTolerance,
    /// The rooms' floor, together.
    floor: f64,
    /// The most circulation the brief allows.
    spare_most: f64,
}

/// A hard constraint not met, and by how much.
#[derive(Clone, Debug, PartialEq)]
enum Violation {
    TooDeep(f64),
    Area(usize),
    Narrow(usize, f64),
    Elongated(usize, f64),
    NotAdjacent(usize, usize, f64),
    Unreachable(usize),
    NoEntrance(f64),
}

/// A slicing sized and judged.
#[derive(Clone, Debug)]
struct Eval {
    outline: Rect,
    rects: Vec<Rect>,
    inner: Vec<Rect>,
    doors: Vec<(usize, usize)>,
    violations: Vec<Violation>,
    hard: f64,
    soft: f64,
    /// Whether the plan has a circulation space.
    spare: bool,
}

impl Eval {
    /// What the annealing descends: the hard violations weighed far above
    /// the soft terms, so the search can cross infeasible plans on its way.
    fn energy(&self) -> f64 {
        HARD * self.hard + self.soft
    }

    /// Whether this is the better plan, lexicographically: fewer hard
    /// violations first, then lower soft energy. Any plan that keeps every
    /// hard constraint beats any that does not, whatever their soft terms.
    fn better_than(&self, other: &Eval) -> bool {
        (self.hard, self.soft) < (other.hard, other.soft)
    }
}

impl<'a> Problem<'a> {
    /// The problem a brief poses; or its refusal, when it cannot be read or
    /// a necessary condition proves it has no plan.
    fn new(spec: &'a Spec) -> Result<Self, Refusal> {
        spec.check().map_err(|e| Refusal::Invalid(e.to_string()))?;
        let s = spec.setbacks;
        let buildable = Rect::new(
            s.left,
            s.front,
            spec.terrain.width - s.left - s.right,
            spec.terrain.depth - s.front - s.back,
        );
        let te = spec.exterior_wall;
        let reasons = necessary(spec, buildable);
        if !reasons.is_empty() {
            return Err(Refusal::Infeasible { reasons });
        }
        Ok(Self {
            spec,
            n: spec.rooms.len(),
            buildable,
            te,
            ti: spec.interior_wall,
            door_span: spec.door + 2.0 * DOOR_CLEARANCE + te,
            entrance: spec.entrance(),
            pairs: spec.adjacent_pairs(),
            floor: spec.rooms.iter().map(|r| r.area).sum(),
            spare_most: spec.max_circulation * spec.rooms.iter().map(|r| r.area).sum::<f64>(),
            tolerance: LinearTolerance::DEFAULT
                .at_magnitude(spec.terrain.width.max(spec.terrain.depth)),
        })
    }

    /// The outline's widths that fit the buildable area, for this much
    /// floor on centrelines.
    fn widths(&self, total: f64) -> (f64, f64) {
        let lo = total / (self.buildable.h - self.te);
        let hi = self.buildable.w - self.te;
        (lo, hi)
    }

    /// A state of the search sized so each room's floor is its area as
    /// nearly as the walls allow, and judged.
    fn evaluate(&self, state: &State) -> Eval {
        let tree = Tree::of(&state.expr);
        let targets: Vec<f64> = self.spec.rooms.iter().map(|r| r.area).collect();
        let mut gross: Vec<f64> = targets.iter().map(|a| a * 1.15).collect();
        gross.push(state.spare);
        let mut placed = self.place(&tree, &gross, state.width);
        for _ in 0..SIZING_PASSES {
            for (i, target) in targets.iter().enumerate() {
                let floor = placed.1[i].area();
                gross[i] = (gross[i] + target - floor).max(target * 0.5);
            }
            placed = self.place(&tree, &gross, state.width);
        }
        let (outline, inner, rects) = (placed.0, placed.1, placed.2);
        self.judge(outline, rects, inner, state.spare > 0.0)
    }

    /// The outline and each room's rectangle and floor, for these gross
    /// areas: each cut in proportion to the floor on either side.
    fn place(&self, tree: &Tree, gross: &[f64], width: f64) -> (Rect, Vec<Rect>, Vec<Rect>) {
        let total: f64 = gross.iter().sum();
        let (lo, hi) = self.widths(total);
        let w = if lo <= hi { width.clamp(lo, hi) } else { hi };
        let h = total / w;
        let footprint_w = w + self.te;
        let x0 = self.buildable.x + (self.buildable.w - footprint_w) / 2.0;
        let outline = Rect::new(x0 + self.te / 2.0, self.buildable.y + self.te / 2.0, w, h);
        let mut rects = vec![Rect::new(0.0, 0.0, 0.0, 0.0); self.n + 1];
        tree.place(tree.root, outline, gross, &mut rects);
        let inner = rects
            .iter()
            .map(|r| {
                let side = |on: bool| if on { self.te / 2.0 } else { self.ti / 2.0 };
                let t = self.tolerance;
                r.inset(
                    side(t.eq(r.x, outline.x)),
                    side(t.eq(r.right(), outline.right())),
                    side(t.eq(r.y, outline.y)),
                    side(t.eq(r.top(), outline.top())),
                )
            })
            .collect();
        (outline, inner, rects)
    }

    /// The constraints and the soft terms of a sized slicing; the space
    /// after the rooms is circulation when `spare`, and nothing otherwise.
    fn judge(&self, outline: Rect, rects: Vec<Rect>, inner: Vec<Rect>, spare: bool) -> Eval {
        let t = self.tolerance;
        let mut violations = Vec::new();
        let deeper = outline.h + self.te - self.buildable.h;
        if deeper > t.value() {
            violations.push(Violation::TooDeep(deeper));
        }
        let mut soft = 0.0;
        for (i, room) in self.spec.rooms.iter().enumerate() {
            let floor = inner[i];
            let error = (floor.area() - room.area).abs() / room.area;
            if error > AREA_TOLERANCE {
                violations.push(Violation::Area(i));
            }
            soft += W_AREA * error;
            let narrow = floor.w.min(floor.h);
            if narrow < room.min_width - t.value() {
                violations.push(Violation::Narrow(i, room.min_width - narrow));
            }
            let aspect = floor.aspect();
            if aspect > room.max_aspect + 1e-9 {
                violations.push(Violation::Elongated(i, aspect - room.max_aspect));
            }
            if aspect.is_finite() {
                soft += W_RATIO * aspect.ln();
            }
            if room.exterior && rects[i].longest_outside(&outline, t) < self.door_span {
                soft += W_PREFERENCES;
            }
        }
        let spaces = if spare { self.n + 1 } else { self.n };
        if spare {
            let passage = inner[self.n].w.min(inner[self.n].h);
            if passage < self.spec.door - t.value() {
                violations.push(Violation::Narrow(self.n, self.spec.door - passage));
            }
            soft += W_SPARE * inner[self.n].area() / self.floor;
        }
        // The walls long enough for a door.
        let mut touching = vec![Vec::new(); spaces];
        for a in 0..spaces {
            for b in a + 1..spaces {
                if rects[a].shared(&rects[b], t) >= self.door_span {
                    touching[a].push(b);
                    touching[b].push(a);
                }
            }
        }
        let mut doors: Vec<(usize, usize)> = Vec::new();
        for &(a, b) in &self.pairs {
            let shared = rects[a].shared(&rects[b], t);
            if shared < self.door_span {
                violations.push(Violation::NotAdjacent(a, b, self.door_span - shared));
            } else {
                doors.push((a.min(b), a.max(b)));
            }
        }
        let outside = rects[self.entrance].longest_outside(&outline, t);
        if outside < self.door_span {
            violations.push(Violation::NoEntrance(self.door_span - outside));
        }
        // Every room reached from the entrance, through the doors asked for
        // first, then the widest of the other walls, breadth first.
        let mut depth = vec![usize::MAX; spaces];
        depth[self.entrance] = 0;
        let mut queue = std::collections::VecDeque::from([self.entrance]);
        while let Some(a) = queue.pop_front() {
            let mut next: Vec<usize> = touching[a]
                .iter()
                .copied()
                .filter(|&b| depth[b] == usize::MAX)
                .collect();
            next.sort_by_key(|&b| {
                let asked = doors.contains(&(a.min(b), a.max(b)));
                (!asked, b)
            });
            for b in next {
                depth[b] = depth[a] + 1;
                let pair = (a.min(b), a.max(b));
                if !doors.contains(&pair) {
                    doors.push(pair);
                }
                queue.push_back(b);
            }
        }
        let mut reached = 0usize;
        let mut depths = 0usize;
        for (i, d) in depth.iter().enumerate() {
            if *d == usize::MAX {
                violations.push(Violation::Unreachable(i));
            } else {
                reached += 1;
                depths += d;
            }
        }
        soft += W_CONNECTIVITY * depths as f64 / reached.max(1) as f64;
        doors.sort_unstable();
        let hard = violations
            .iter()
            .map(|v| match v {
                Violation::TooDeep(d) | Violation::NoEntrance(d) => 1.0 + d,
                Violation::Narrow(_, d) | Violation::Elongated(_, d) => 1.0 + d,
                Violation::NotAdjacent(_, _, d) => 1.0 + d,
                Violation::Area(_) | Violation::Unreachable(_) => 1.0,
            })
            .sum();
        Eval {
            outline,
            rects,
            inner,
            doors,
            violations,
            hard,
            soft,
            spare,
        }
    }

    /// One annealing run from `seed`: the best state it met, sized —
    /// best lexicographically, its hard violations first.
    fn anneal(&self, seed: u64, circulation: bool) -> Eval {
        let mut rng = Rng(seed ^ 0x5EED_CAD5_1A7E_0000);
        let mut order: Vec<usize> = (0..self.n).collect();
        for i in (1..self.n).rev() {
            order.swap(i, rng.below(i + 1));
        }
        // The circulation space among the rooms, empty to begin with.
        order.insert(rng.below(self.n + 1), self.n);
        let mut expr = Vec::with_capacity(2 * self.n + 2);
        let mut cut = if rng.unit() < 0.5 { Tok::V } else { Tok::H };
        for (k, room) in order.iter().enumerate() {
            expr.push(Tok::Room(*room));
            if k > 0 {
                expr.push(cut);
                cut = if cut == Tok::V { Tok::H } else { Tok::V };
            }
        }
        let floor: f64 = self.floor * 1.2;
        let (lo, hi) = self.widths(floor);
        let width = if lo <= hi {
            (floor * self.buildable.w / self.buildable.h)
                .sqrt()
                .clamp(lo, hi)
        } else {
            hi
        };
        let mut state = State {
            expr,
            width,
            spare: 0.0,
        };
        let mut current = self.evaluate(&state);
        let mut best = current.clone();
        let steps = STEPS_PER_ROOM * self.n.max(2);
        for step in 0..steps {
            let temperature = T_START * (T_END / T_START).powf(step as f64 / steps as f64);
            let mut candidate = state.clone();
            if !self.perturb(&mut candidate, circulation, &mut rng) {
                continue;
            }
            let judged = self.evaluate(&candidate);
            let delta = judged.energy() - current.energy();
            if delta <= 0.0 || rng.unit() < (-delta / temperature).exp() {
                state = candidate;
                current = judged;
                if current.better_than(&best) {
                    best = current.clone();
                }
            }
        }
        best
    }

    /// One move of the search; false when it would break the expression's
    /// form, and nothing changed.
    fn perturb(&self, state: &mut State, circulation: bool, rng: &mut Rng) -> bool {
        let expr = &mut state.expr;
        let operands: Vec<usize> = (0..expr.len()).filter(|&i| !expr[i].is_cut()).collect();
        match rng.below(12) {
            // The circulation space made, grown, shrunk, or taken away.
            10 | 11 if circulation && self.spare_most >= SPARE_LEAST => {
                let spare = &mut state.spare;
                *spare = if *spare == 0.0 {
                    SPARE_LEAST + rng.unit() * self.floor * 0.1
                } else if rng.unit() < 0.25 {
                    0.0
                } else {
                    *spare * (0.3 * (rng.unit() * 2.0 - 1.0)).exp()
                };
                if *spare < SPARE_LEAST {
                    *spare = 0.0;
                }
                *spare = spare.min(self.spare_most);
                true
            }
            // M1: two neighbouring rooms swapped.
            0..=2 => {
                let k = rng.below(operands.len() - 1);
                expr.swap(operands[k], operands[k + 1]);
                true
            }
            // Any two rooms swapped.
            3 => {
                let a = rng.below(operands.len());
                let b = rng.below(operands.len());
                expr.swap(operands[a], operands[b]);
                a != b
            }
            // M2: a chain of cuts turned the other way.
            4 | 5 => {
                let cuts: Vec<usize> = (0..expr.len()).filter(|&i| expr[i].is_cut()).collect();
                let mut i = cuts[rng.below(cuts.len())];
                while i > 0 && expr[i - 1].is_cut() {
                    i -= 1;
                }
                while i < expr.len() && expr[i].is_cut() {
                    expr[i] = if expr[i] == Tok::V { Tok::H } else { Tok::V };
                    i += 1;
                }
                true
            }
            // M3: a room and a cut beside it swapped, if the result is
            // still a normalised expression.
            6 | 7 => {
                let i = rng.below(expr.len() - 1);
                if expr[i].is_cut() == expr[i + 1].is_cut() {
                    return false;
                }
                expr.swap(i, i + 1);
                if normalised(expr) {
                    true
                } else {
                    expr.swap(i, i + 1);
                    false
                }
            }
            // M4: the outline wider or narrower.
            _ => {
                state.width *= (0.15 * (rng.unit() * 2.0 - 1.0)).exp();
                true
            }
        }
    }

    fn layout(&self, eval: Eval, seed: u64) -> Layout {
        let spare = eval.spare;
        let footprint = Rect::new(
            eval.outline.x - self.te / 2.0,
            eval.outline.y - self.te / 2.0,
            eval.outline.w + self.te,
            eval.outline.h + self.te,
        );
        let mut rooms: Vec<PlacedRoom> = self
            .spec
            .rooms
            .iter()
            .enumerate()
            .map(|(i, room)| PlacedRoom {
                name: room.name.clone(),
                rect: eval.rects[i],
                inner: eval.inner[i],
            })
            .collect();
        if spare {
            rooms.push(PlacedRoom {
                name: CIRCULATION.to_owned(),
                rect: eval.rects[self.n],
                inner: eval.inner[self.n],
            });
        }
        Layout {
            footprint,
            outline: eval.outline,
            rooms,
            circulation: spare.then_some(self.n),
            entrance: self.entrance,
            doors: eval.doors,
            energy: eval.soft,
            seed,
        }
    }

    /// The violations of the nearest plan found, in words.
    fn explain(&self, eval: &Eval) -> Vec<String> {
        let name = |i: usize| {
            self.spec
                .rooms
                .get(i)
                .map_or(CIRCULATION, |r| r.name.as_str())
        };
        let mut reasons: Vec<String> = Vec::new();
        for violation in &eval.violations {
            let reason = match violation {
                Violation::TooDeep(_) => {
                    "The rooms do not fit within the buildable area at any shape tried.".to_owned()
                }
                Violation::Area(i) => format!(
                    "The {}'s area could not be kept within 5% of {} m².",
                    name(*i),
                    short(self.spec.rooms[*i].area)
                ),
                Violation::Narrow(i, _) => format!(
                    "The {} could not be made {} m wide.",
                    name(*i),
                    short(
                        self.spec
                            .rooms
                            .get(*i)
                            .map_or(self.spec.door, |r| r.min_width)
                    )
                ),
                Violation::Elongated(i, _) => format!(
                    "The {} could not be kept within its aspect ratio of {}.",
                    name(*i),
                    short(self.spec.rooms[*i].max_aspect)
                ),
                Violation::NotAdjacent(a, b, _) => format!(
                    "The {} and the {} could not share a wall long enough for a door.",
                    name(*a),
                    name(*b)
                ),
                Violation::Unreachable(i) => format!(
                    "The {} could not be reached from the {}.",
                    name(*i),
                    name(self.entrance)
                ),
                Violation::NoEntrance(_) => format!(
                    "The {} could not have an outside wall for the entrance.",
                    name(self.entrance)
                ),
            };
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        reasons
    }
}

/// No two of the same cut in a row, and every prefix holds more rooms
/// than cuts: the form Wong and Liu's moves keep.
fn normalised(expr: &[Tok]) -> bool {
    let mut rooms = 0usize;
    let mut cuts = 0usize;
    for (i, tok) in expr.iter().enumerate() {
        if tok.is_cut() {
            cuts += 1;
            if cuts >= rooms {
                return false;
            }
            if i > 0 && expr[i - 1] == *tok {
                return false;
            }
        } else {
            rooms += 1;
        }
    }
    true
}

/// A slicing as a tree, from its expression.
#[derive(Debug)]
struct Tree {
    nodes: Vec<Node>,
    root: usize,
}

#[derive(Debug)]
enum Node {
    Room(usize),
    Cut { vertical: bool, a: usize, b: usize },
}

impl Tree {
    fn of(expr: &[Tok]) -> Self {
        let mut nodes = Vec::with_capacity(expr.len());
        let mut stack = Vec::new();
        for tok in expr {
            let node = match tok {
                Tok::Room(r) => Node::Room(*r),
                cut => {
                    let b = stack.pop().expect("a normalised expression");
                    let a = stack.pop().expect("a normalised expression");
                    Node::Cut {
                        vertical: *cut == Tok::V,
                        a,
                        b,
                    }
                }
            };
            nodes.push(node);
            stack.push(nodes.len() - 1);
        }
        let root = stack.pop().expect("one room at least");
        Self { nodes, root }
    }

    fn weight(&self, node: usize, gross: &[f64]) -> f64 {
        match self.nodes[node] {
            Node::Room(r) => gross[r],
            Node::Cut { a, b, .. } => self.weight(a, gross) + self.weight(b, gross),
        }
    }

    /// Each room's rectangle within `rect`: the first operand left of a
    /// vertical cut, below a horizontal one.
    fn place(&self, node: usize, rect: Rect, gross: &[f64], out: &mut [Rect]) {
        match self.nodes[node] {
            Node::Room(r) => out[r] = rect,
            Node::Cut { vertical, a, b } => {
                let (wa, wb) = (self.weight(a, gross), self.weight(b, gross));
                let share = wa / (wa + wb);
                let (first, second) = if vertical {
                    let w = rect.w * share;
                    (
                        Rect::new(rect.x, rect.y, w, rect.h),
                        Rect::new(rect.x + w, rect.y, rect.w - w, rect.h),
                    )
                } else {
                    let h = rect.h * share;
                    (
                        Rect::new(rect.x, rect.y, rect.w, h),
                        Rect::new(rect.x, rect.y + h, rect.w, rect.h - h),
                    )
                };
                self.place(a, first, gross, out);
                self.place(b, second, gross, out);
            }
        }
    }
}

/// A number as a drafter says it: to the centimetre, without trailing zeros.
fn short(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The searches a plan of `rooms` rooms is given: forty for up to six
/// rooms, then fewer, down to eight.
pub fn searches_for(rooms: usize) -> usize {
    (STEP_BUDGET / (STEPS_PER_ROOM * rooms.max(1))).clamp(SEARCHES.0, SEARCHES.1)
}

/// The best plan for `spec`, from its seed and the ones after it, within
/// [`searches_for`] its rooms; or why there is none.
pub fn solve(spec: &Spec) -> Result<Layout, Refusal> {
    alternatives(spec, 1).map(|mut plans| plans.swap_remove(0))
}

/// Up to `count` different plans, best first, within `count ×`
/// [`searches_for`] its rooms; or why there is none.
pub fn alternatives(spec: &Spec, count: usize) -> Result<Vec<Layout>, Refusal> {
    alternatives_within(spec, count, count.max(1) * searches_for(spec.rooms.len()))
}

/// Up to `count` different plans, best first, from the brief's seed and
/// those after it, stopping when `count` are found or after `searches`.
/// The first quarter of the searches make no circulation space; the rest
/// may, within the brief's budget, if the rooms alone gave too few plans.
/// Ranked lexicographically: a plan is kept only when it keeps every hard
/// constraint; plans without circulation come before plans with it; then
/// by soft energy.
pub fn alternatives_within(
    spec: &Spec,
    count: usize,
    searches: usize,
) -> Result<Vec<Layout>, Refusal> {
    let problem = Problem::new(spec)?;
    let searches = searches.max(1);
    let mut found: Vec<Layout> = Vec::new();
    let mut nearest: Option<Eval> = None;
    for k in 0..searches {
        let seed = spec.seed.wrapping_add(k as u64);
        let rooms_alone = (k as f64) < (searches as f64 * WITHOUT_CIRCULATION).ceil();
        let eval = problem.anneal(seed, !rooms_alone);
        if eval.violations.is_empty() {
            let layout = problem.layout(eval, seed);
            if !found.iter().any(|other| same_plan(other, &layout)) {
                found.push(layout);
            }
        } else if nearest.as_ref().is_none_or(|n| eval.better_than(n)) {
            nearest = Some(eval);
        }
        if found.len() >= count.max(1) {
            break;
        }
    }
    if found.is_empty() {
        let eval = nearest.expect("a search at least");
        return Err(Refusal::Exhausted {
            reasons: problem.explain(&eval),
            searches,
        });
    }
    found.sort_by(|a, b| {
        (a.circulation.is_some(), a.energy)
            .partial_cmp(&(b.circulation.is_some(), b.energy))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    found.truncate(count.max(1));
    Ok(found)
}

/// What a brief needs of any plan, proved before a search: each sentence a
/// condition no plan can meet. Every bound allows the ±5% a room's area may
/// be off, and the most wall a room's rectangle can have.
fn necessary(spec: &Spec, buildable: Rect) -> Vec<String> {
    let te = spec.exterior_wall;
    // The outline's inside: what every room's floor lies in.
    let (wide, deep) = (buildable.w - 2.0 * te, buildable.h - 2.0 * te);
    if wide <= 0.0 || deep <= 0.0 {
        return vec!["The setbacks leave no room to build.".to_owned()];
    }
    let (narrow, long) = (wide.min(deep), wide.max(deep));
    let mut reasons = Vec::new();
    let least: f64 = spec
        .rooms
        .iter()
        .map(|r| r.area * (1.0 - AREA_TOLERANCE))
        .sum();
    if least > wide * deep {
        reasons.push(format!(
            "The rooms need at least {} m² inside their walls; the buildable area holds at most {} m².",
            short(least),
            short(wide * deep)
        ));
    }
    // A room's rectangle is its floor and at least the thinner wall's half
    // on every side, so it covers at least (√floor + t)², and the rooms
    // together at least the sum: more than the outline can be is too much.
    let thinner = te.min(spec.interior_wall);
    let walled: f64 = spec
        .rooms
        .iter()
        .map(|r| ((r.area * (1.0 - AREA_TOLERANCE)).sqrt() + thinner).powi(2))
        .sum();
    let outline = (buildable.w - te) * (buildable.h - te);
    if least <= wide * deep && walled > outline {
        reasons.push(format!(
            "With their walls the rooms cover at least {} m²; the building can cover at most {} m².",
            short(walled),
            short(outline)
        ));
    }
    let span = spec.door + 2.0 * DOOR_CLEARANCE + te;
    let pairs = spec.adjacent_pairs();
    for (i, room) in spec.rooms.iter().enumerate() {
        let most = room.area * (1.0 + AREA_TOLERANCE);
        if most < room.min_width * room.min_width {
            reasons.push(format!(
                "The {}'s {} m² cannot be {} m wide both ways.",
                room.name,
                short(room.area),
                short(room.min_width)
            ));
            continue;
        }
        if room.min_width > narrow {
            reasons.push(format!(
                "The {} cannot be {} m wide: the buildable area is {} m across inside its walls.",
                room.name,
                short(room.min_width),
                short(narrow)
            ));
            continue;
        }
        // The largest floor a rectangle of this aspect has inside the
        // outline's inside.
        let largest = narrow * long.min(room.max_aspect * narrow);
        if room.area * (1.0 - AREA_TOLERANCE) > largest {
            reasons.push(format!(
                "The {} cannot have {} m² within its aspect ratio of {} on the buildable area.",
                room.name,
                short(room.area),
                short(room.max_aspect)
            ));
            continue;
        }
        // Each door, and the entrance's, takes a stretch of its walls of
        // its own; a rectangle's edge is longest at its narrowest.
        let doors = pairs.iter().filter(|(a, b)| *a == i || *b == i).count()
            + usize::from(i == spec.entrance());
        let short_side = room.min_width.max((most / room.max_aspect).sqrt());
        let edge = 2.0 * (short_side + most / short_side + 2.0 * te);
        if doors as f64 * span > edge {
            reasons.push(format!(
                "The {} has too little wall for its {} doors: each needs {} m.",
                room.name,
                doors,
                short(span)
            ));
        }
    }
    reasons
}

/// Plans whose rooms are all within this of each other are the same plan:
/// a centimetre, the precision a drafter reads a plan to.
const SAME_PLAN: f64 = 0.01;

/// Two plans with every room in the same place.
fn same_plan(a: &Layout, b: &Layout) -> bool {
    a.rooms.iter().zip(&b.rooms).all(|(p, q)| {
        (p.rect.x - q.rect.x).abs() < SAME_PLAN
            && (p.rect.y - q.rect.y).abs() < SAME_PLAN
            && (p.rect.w - q.rect.w).abs() < SAME_PLAN
            && (p.rect.h - q.rect.h).abs() < SAME_PLAN
    })
}
