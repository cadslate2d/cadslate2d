//! Placement (#273): circles and arcs tangent to, or concentric with,
//! picked geometry, and copies laid out along a curve or every so far
//! inside a boundary.
//!
//! What is placed is constrained where it can be (ADR-0038): a tangent
//! circle or arc keeps its tangencies — an arc its ends on the curves it
//! touches too — a concentric circle its centre, and the circles of a path
//! array their places on the path and their size. The constraints are
//! added in the transaction that makes the geometry, the picked objects
//! held where they are: one undo. CIRCLE's `Ttr` and `TTT` place through
//! [`commit_tangent`] as `TANARC` does.

use cad_core::{Aabb, EntityId, Point2, Vec2};
use cad_document::{
    ArcEntity, CircleEntity, Constraint, Entity, Kind, Part, Reference, Touch, TransactionBuilder,
};
use cad_geometry::tangent::{Curve, between_parallels, nearest_to_picks, tangent_to_two};
use cad_geometry::{Circle, Path, Transform, ring_contains};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::constrain::edge_at;
use super::hatch::{candidates_without, region_at, rings_of};
use super::modify::{SELECT_OBJECTS, adopt_selection, selection_expected};
use super::points::{from_end_nearest, path_of};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const DIAMETER: Keyword = Keyword::new("Diameter");
const MEASURE: Keyword = Keyword::new("Measure");
/// The most items an array along a path or over a grid makes.
const ITEM_LIMIT: usize = 10_000;

/// A curve picked for a tangency: the object, the curve its piece nearest
/// the pick lies on — a line as the whole line, an arc as its circle —
/// and where it was picked.
#[derive(Clone, Copy, Debug)]
pub(super) struct Touched {
    pub entity: EntityId,
    pub curve: Curve,
    pub at: Point2,
}

/// The curve picked at `at` on `entity`, or `None`, said, when it has
/// nothing to touch.
pub(super) fn touched(
    ctx: &mut CommandContext<'_>,
    entity: EntityId,
    at: Point2,
) -> Option<Touched> {
    let nearest = ctx.drawing.entity(entity).and_then(|picked| {
        picked
            .primitives(ctx.drawing.tables())
            .into_iter()
            .min_by(|a, b| {
                a.closest_point(at)
                    .distance_to(at)
                    .total_cmp(&b.closest_point(at).distance_to(at))
            })
    });
    match nearest {
        Some(primitive) => Some(Touched {
            entity,
            curve: Curve::of(&primitive),
            at,
        }),
        None => {
            ctx.message("That object has no line, arc, or circle to touch.");
            None
        }
    }
}

/// The circle of `radius` tangent to two picked curves that a drafter
/// means: of those there are, the one whose touching points are nearest
/// the picks; between two parallel lines its diameter apart, the one
/// level with the picks.
pub(super) fn tangent_of_radius(first: &Touched, second: &Touched, radius: f64) -> Option<Circle> {
    let found = tangent_to_two(&first.curve, &second.curve, radius);
    let between = first.at + (second.at - first.at) * 0.5;
    meant(&found, &[*first, *second])
        .or_else(|| between_parallels(&first.curve, &second.curve, radius, between))
}

/// Of the circles tangent to the picked curves, the one a drafter means:
/// its touching points nearest the picks.
pub(super) fn meant(candidates: &[Circle], touched: &[Touched]) -> Option<Circle> {
    let curves: Vec<Curve> = touched.iter().map(|t| t.curve).collect();
    let picks: Vec<Point2> = touched.iter().map(|t| t.at).collect();
    nearest_to_picks(candidates, &curves, &picks)
}

/// The tangencies `made` can be constrained by: each picked object's edge
/// that a constraint can name, once, how `made` touches it, and the curve.
fn tangencies(
    ctx: &CommandContext<'_>,
    made: &Entity,
    touched: &[Touched],
) -> Vec<(Reference, Touch, Curve)> {
    let mut found: Vec<(Reference, Touch, Curve)> = Vec::new();
    for t in touched {
        let Some(picked) = ctx.drawing.entity(t.entity) else {
            continue;
        };
        let Some(edge) = edge_at(picked, t.entity, t.at) else {
            continue;
        };
        let Some(touch) = Touch::between((made, Part::Edge), (picked, edge.part)) else {
            continue;
        };
        if found.iter().all(|(seen, _, _)| *seen != edge) {
            found.push((edge, touch, t.curve));
        }
    }
    found
}

/// Adds `constraint`, unless the drawing cannot take it — then the
/// geometry is placed without it.
fn constrain(transaction: &mut TransactionBuilder<'_>, constraint: Constraint) {
    let _ = transaction.add_constraint(constraint);
}

/// Inserts `made`, which touches the picked curves, tangent to each that
/// can take the constraint — an arc with its ends on them as well, each on
/// the one it touches — the picked objects held still.
pub(super) fn commit_tangent(ctx: &mut CommandContext<'_>, made: Entity, touched: &[Touched]) {
    let tangencies = tangencies(ctx, &made, touched);
    // An arc's start on the curve it touches there, its end on the other.
    let ends = match (&made, tangencies.as_slice()) {
        (Entity::Arc(arc), [a, b]) => {
            let start = arc.arc().start_point();
            if a.2.distance_to(start) <= b.2.distance_to(start) {
                vec![(Part::Start, a.0), (Part::End, b.0)]
            } else {
                vec![(Part::Start, b.0), (Part::End, a.0)]
            }
        }
        _ => Vec::new(),
    };
    let mut transaction = ctx.transaction();
    let id = transaction
        .insert(made)
        .expect("a circle or an arc has no style to miss");
    let edge = Reference::new(id, Part::Edge);
    for (picked, touch, _) in &tangencies {
        constrain(
            &mut transaction,
            Constraint::geometric(Kind::Tangent { touch: *touch }, vec![edge, *picked]),
        );
        transaction.hold(picked.entity);
    }
    for (end, picked) in ends {
        constrain(
            &mut transaction,
            Constraint::geometric(Kind::Coincident, vec![Reference::new(id, end), picked]),
        );
    }
    transaction.commit();
}

/// The arc of `circle` between where it touches `first` and where it
/// touches `second`, the shorter way round: a fillet's arc. `None` when
/// the two touch at one point.
fn arc_between(circle: Circle, first: &Curve, second: &Curve) -> Option<ArcEntity> {
    let a = first.touching_point(&circle)?;
    let b = second.touching_point(&circle)?;
    let angle = |p: Point2| (p.y - circle.center.y).atan2(p.x - circle.center.x);
    let (from, to) = (angle(a), angle(b));
    let sweep = (to - from).rem_euclid(std::f64::consts::TAU);
    if a.distance_to(b) <= 1e-9 * circle.radius.max(1.0) {
        return None;
    }
    // Counter-clockwise from the first when that is the shorter way.
    Some(if sweep <= std::f64::consts::PI {
        ArcEntity::new(circle.center, circle.radius, from, to)
    } else {
        ArcEntity::new(circle.center, circle.radius, to, from)
    })
}

// ── TANARC ──

#[derive(Debug)]
enum TanArcState {
    Picking(Vec<Touched>),
    Radius([Touched; 2]),
}

/// `TANARC`: an arc of a radius tangent to two objects, between where it
/// touches them — a fillet that trims nothing — kept tangent, its ends on
/// them.
#[derive(Debug)]
pub(crate) struct TanArcCommand {
    state: TanArcState,
}

impl TanArcCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: TanArcState::Picking(Vec::new()),
        }
    }
}

impl Command for TanArcCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match &self.state {
            TanArcState::Picking(picked) if picked.is_empty() => {
                "Select first object for the arc to touch"
            }
            TanArcState::Picking(_) => "Select second object for the arc to touch",
            TanArcState::Radius(_) => "Specify radius of arc",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            TanArcState::Picking(_) => ExpectedInput::object(),
            TanArcState::Radius(_) => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&mut self.state, input) {
            (_, CommandInput::Accept) => return Step::Finished,
            (TanArcState::Picking(picked), CommandInput::Pick { entity, at }) => {
                if let Some(t) = touched(ctx, entity, at) {
                    picked.push(t);
                    if let [first, second] = picked.as_slice() {
                        self.state = TanArcState::Radius([*first, *second]);
                    }
                }
            }
            (TanArcState::Radius(touched), CommandInput::Distance(radius)) => {
                if !(radius.is_finite() && radius > 0.0) {
                    ctx.message("The radius must be greater than zero.");
                    return Step::Continue;
                }
                let [first, second] = *touched;
                let arc = tangent_of_radius(&first, &second, radius)
                    .and_then(|circle| arc_between(circle, &first.curve, &second.curve));
                let Some(arc) = arc else {
                    ctx.message("Arc does not exist.");
                    return Step::Continue;
                };
                commit_tangent(ctx, arc.into(), &[first, second]);
                return Step::Finished;
            }
            _ => {}
        }
        Step::Continue
    }
}

// ── CONCENTRIC ──

#[derive(Debug)]
enum ConcentricState {
    Pick,
    Radius { of: Reference, center: Point2 },
    Diameter { of: Reference, center: Point2 },
}

/// `CONCENTRIC`: a circle about the centre of a picked circle or arc,
/// kept concentric with it.
#[derive(Debug)]
pub(crate) struct ConcentricCommand {
    state: ConcentricState,
}

impl ConcentricCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: ConcentricState::Pick,
        }
    }

    fn commit(of: Reference, center: Point2, radius: f64, ctx: &mut CommandContext<'_>) -> Step {
        if !(radius.is_finite() && radius > 0.0) {
            ctx.message("The radius must be greater than zero.");
            return Step::Continue;
        }
        let mut transaction = ctx.transaction();
        let id = transaction
            .insert(CircleEntity::new(center, radius))
            .expect("a circle has no style to miss");
        constrain(
            &mut transaction,
            Constraint::geometric(Kind::Concentric, vec![Reference::new(id, Part::Edge), of]),
        );
        transaction.hold(of.entity);
        transaction.commit();
        Step::Finished
    }
}

impl Command for ConcentricCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match self.state {
            ConcentricState::Pick => "Select circle or arc to be concentric with",
            ConcentricState::Radius { .. } => "Specify radius of circle",
            ConcentricState::Diameter { .. } => "Specify diameter of circle",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            ConcentricState::Pick => ExpectedInput::object(),
            ConcentricState::Radius { .. } => ExpectedInput::distance().with_keywords(&[DIAMETER]),
            ConcentricState::Diameter { .. } => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (_, CommandInput::Accept) => Step::Finished,
            (ConcentricState::Pick, CommandInput::Pick { entity, .. }) => {
                let center = match ctx.drawing.entity(entity) {
                    Some(Entity::Circle(circle)) => Some(circle.center),
                    Some(Entity::Arc(arc)) => Some(arc.center),
                    _ => None,
                };
                match center {
                    Some(center) => {
                        self.state = ConcentricState::Radius {
                            of: Reference::new(entity, Part::Edge),
                            center,
                        };
                    }
                    None => ctx.message("Select a circle or an arc."),
                }
                Step::Continue
            }
            (ConcentricState::Radius { of, center }, CommandInput::Keyword(k)) if k == DIAMETER => {
                self.state = ConcentricState::Diameter {
                    of: *of,
                    center: *center,
                };
                Step::Continue
            }
            (ConcentricState::Radius { of, center }, CommandInput::Distance(radius)) => {
                Self::commit(*of, *center, radius, ctx)
            }
            (ConcentricState::Diameter { of, center }, CommandInput::Distance(diameter)) => {
                Self::commit(*of, *center, diameter / 2.0, ctx)
            }
            _ => Step::Continue,
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            ConcentricState::Pick => None,
            ConcentricState::Radius { center, .. } | ConcentricState::Diameter { center, .. } => {
                Some(center)
            }
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let radius = match self.state {
            ConcentricState::Pick => return,
            ConcentricState::Radius { center, .. } => center.distance_to(cursor),
            ConcentricState::Diameter { center, .. } => center.distance_to(cursor) / 2.0,
        };
        if let Some(center) = self.base_point().filter(|_| radius > 0.0) {
            out.push(OverlayItem::PreviewEntity(Entity::Circle(
                CircleEntity::new(center, radius),
            )));
        }
    }
}

// ── ARRAYPATH and ARRAYGRID ──

/// The point a selection is placed by: a lone circle's or arc's centre, a
/// lone point's place, otherwise the middle of its bounds.
fn reference_point(ctx: &CommandContext<'_>, sources: &[EntityId]) -> Option<Point2> {
    if let [id] = sources {
        match ctx.drawing.entity(*id)? {
            Entity::Circle(circle) => return Some(circle.center),
            Entity::Arc(arc) => return Some(arc.center),
            Entity::Point(point) => return Some(point.position),
            _ => {}
        }
    }
    bounds_of(ctx, sources).map(|bounds| bounds.center())
}

fn bounds_of(ctx: &CommandContext<'_>, sources: &[EntityId]) -> Option<Aabb> {
    sources
        .iter()
        .filter_map(|id| ctx.drawing.entity(*id))
        .map(|entity| ctx.drawing.bounds_of(entity))
        .reduce(|a, b| a.union(&b))
}

/// A whole number of items, from `least` to the limit.
fn item_count(value: f64, least: usize) -> Option<usize> {
    let whole = value.round();
    (value.is_finite() && (value - whole).abs() < 1e-9 && whole >= least as f64)
        .then_some(whole as usize)
        .filter(|count| *count <= ITEM_LIMIT)
}

#[derive(Debug)]
enum PathState {
    Selecting,
    Path,
    Count,
    Spacing,
}

/// `ARRAYPATH`: items of a selection along a curve — so many spread from
/// one end to the other, round a closed one, or every so far — the
/// selection moved to the first. Circles are kept on the path and the
/// size of the first.
#[derive(Debug)]
pub(crate) struct ArrayPathCommand {
    state: PathState,
    sources: Vec<EntityId>,
    path: Option<(EntityId, Path)>,
}

impl ArrayPathCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: PathState::Selecting,
            sources: Vec::new(),
            path: None,
        }
    }

    fn adopt(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match adopt_selection(ctx) {
            Some(edit) => {
                self.sources = edit.sources().to_vec();
                self.state = PathState::Path;
                Step::Continue
            }
            None => Step::Finished,
        }
    }

    /// Distances along `path` for `count` items: end to end on an open
    /// path, evenly round a closed one.
    fn spread(path: &Path, count: usize) -> Vec<f64> {
        let length = path.length();
        let gaps = if path.closed { count } else { count - 1 };
        (0..count)
            .map(|k| length * k as f64 / gaps as f64)
            .collect()
    }

    /// Every `spacing` from the start, as far as the path goes.
    fn every(path: &Path, spacing: f64) -> Vec<f64> {
        let length = path.length();
        let room = length * (1.0 + 1e-12);
        (0..)
            .map(|k| k as f64 * spacing)
            .take_while(|d| {
                if path.closed {
                    *d < length - spacing * 1e-9
                } else {
                    *d <= room
                }
            })
            .take(ITEM_LIMIT + 1)
            .collect()
    }

    fn commit(&self, distances: &[f64], ctx: &mut CommandContext<'_>) -> Step {
        let Some((path_id, path)) = &self.path else {
            return Step::Finished;
        };
        if distances.len() > ITEM_LIMIT {
            ctx.message(format!("That would be more than {ITEM_LIMIT} items."));
            return Step::Continue;
        }
        let Some(base) = reference_point(ctx, &self.sources) else {
            return Step::Finished;
        };
        let places: Vec<Point2> = distances
            .iter()
            .filter_map(|d| path.point_along(d.min(path.length())))
            .collect();
        // A lone circle on a line, an arc, or a circle stays on it.
        let lone_circle = match self.sources.as_slice() {
            [id] => matches!(ctx.drawing.entity(*id), Some(Entity::Circle(_))).then_some(*id),
            _ => None,
        };
        let on = ctx
            .drawing
            .entity(*path_id)
            .filter(|path| matches!(path, Entity::Line(_) | Entity::Arc(_) | Entity::Circle(_)))
            .map(|_| Reference::new(*path_id, Part::Edge));
        let moved: Vec<Vec<(EntityId, Entity)>> = places
            .iter()
            .map(|place| {
                let transform = Transform::translation(*place - base);
                self.sources
                    .iter()
                    .filter_map(|id| Some((*id, ctx.drawing.entity(*id)?.transformed(&transform))))
                    .collect()
            })
            .collect();
        let mut transaction = ctx.transaction();
        let mut items: Vec<EntityId> = Vec::new();
        for (index, item) in moved.into_iter().enumerate() {
            for (source, entity) in item {
                if index == 0 {
                    transaction
                        .replace(source, entity)
                        .expect("the selection is live");
                    items.push(source);
                } else {
                    items.push(
                        transaction
                            .insert_like(source, entity)
                            .expect("the selection is live"),
                    );
                }
            }
        }
        if let (Some(first), Some(on)) = (lone_circle, on) {
            for item in &items {
                constrain(
                    &mut transaction,
                    Constraint::geometric(
                        Kind::Coincident,
                        vec![Reference::new(*item, Part::Center), on],
                    ),
                );
                if *item != first {
                    constrain(
                        &mut transaction,
                        Constraint::geometric(
                            Kind::Equal,
                            vec![
                                Reference::new(first, Part::Edge),
                                Reference::new(*item, Part::Edge),
                            ],
                        ),
                    );
                }
            }
            transaction.hold(*path_id);
        }
        transaction.commit();
        ctx.selection.clear();
        ctx.message(format!("{} items along the path.", places.len()));
        Step::Finished
    }
}

impl Command for ArrayPathCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.adopt(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            PathState::Selecting => Prompt::new(SELECT_OBJECTS),
            PathState::Path => Prompt::new("Select path curve"),
            PathState::Count => Prompt::new("Enter number of items along path"),
            PathState::Spacing => Prompt::new("Specify the distance between items"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            PathState::Selecting => selection_expected(),
            PathState::Path => ExpectedInput::new(InputKind::Object),
            PathState::Count => ExpectedInput::factor().with_keywords(&[MEASURE]),
            PathState::Spacing => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (PathState::Selecting, CommandInput::Accept) => self.adopt(ctx),
            (_, CommandInput::Accept) => Step::Finished,
            (PathState::Path, CommandInput::Pick { entity, at }) => {
                if self.sources.contains(&entity) {
                    ctx.message("The path cannot be one of the objects arrayed.");
                    return Step::Continue;
                }
                match ctx.drawing.entity(entity).and_then(path_of) {
                    Some(path) if path.length() > 0.0 => {
                        let path = from_end_nearest(path, at);
                        self.path = Some((entity, path));
                        self.state = PathState::Count;
                    }
                    _ => ctx.message(
                        "That object is no path: pick a line, arc, circle, ellipse, spline, or polyline.",
                    ),
                }
                Step::Continue
            }
            (PathState::Count, CommandInput::Keyword(k)) if k == MEASURE => {
                self.state = PathState::Spacing;
                Step::Continue
            }
            (PathState::Count, CommandInput::Distance(count)) => match item_count(count, 2) {
                Some(count) => {
                    let path = &self.path.as_ref().expect("a path is picked first").1;
                    let distances = Self::spread(path, count);
                    self.commit(&distances, ctx)
                }
                None => {
                    ctx.message(format!("Enter a whole number of items, 2 to {ITEM_LIMIT}."));
                    Step::Continue
                }
            },
            (PathState::Spacing, CommandInput::Distance(spacing)) => {
                if !(spacing.is_finite() && spacing > 0.0) {
                    ctx.message("The distance between items must be greater than zero.");
                    return Step::Continue;
                }
                let path = &self.path.as_ref().expect("a path is picked first").1;
                let distances = Self::every(path, spacing);
                self.commit(&distances, ctx)
            }
            _ => Step::Continue,
        }
    }
}

#[derive(Debug)]
enum GridState {
    Selecting,
    Inside,
    ColumnSpacing,
    RowSpacing,
}

/// `ARRAYGRID`: copies of a selection on a grid through it, every so far
/// across and up, wherever a copy lies wholly inside the region round a
/// picked point — the smallest loop of what is shown round it, its
/// islands left out, as HATCH finds it.
#[derive(Debug)]
pub(crate) struct ArrayGridCommand {
    state: GridState,
    sources: Vec<EntityId>,
    rings: Vec<Vec<Point2>>,
    column_spacing: f64,
}

impl ArrayGridCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: GridState::Selecting,
            sources: Vec::new(),
            rings: Vec::new(),
            column_spacing: 0.0,
        }
    }

    fn adopt(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match adopt_selection(ctx) {
            Some(edit) => {
                self.sources = edit.sources().to_vec();
                self.state = GridState::Inside;
                Step::Continue
            }
            None => Step::Finished,
        }
    }

    fn inside(&self, point: Point2) -> bool {
        self.rings
            .iter()
            .filter(|ring| ring_contains(ring, point))
            .count()
            % 2
            == 1
    }

    /// The offsets of the grid's places, but the selection's own, at
    /// which a copy lies inside the region.
    fn offsets(&self, bounds: Aabb, across: f64, up: f64) -> Result<Vec<Vec2>, String> {
        let region = self
            .rings
            .iter()
            .flatten()
            .fold(None::<Aabb>, |acc, p| {
                let b = Aabb::from_points(*p, *p);
                Some(acc.map_or(b, |a| a.union(&b)))
            })
            .ok_or("The boundary has no extent.")?;
        let range = |low: f64, high: f64, step: f64| {
            ((low / step).ceil() as i64, (high / step).floor() as i64)
        };
        let (left, right) = range(
            region.min().x - bounds.max().x,
            region.max().x - bounds.min().x,
            across,
        );
        let (bottom, top) = range(
            region.min().y - bounds.max().y,
            region.max().y - bounds.min().y,
            up,
        );
        let places = (right - left + 1).max(0) as f64 * (top - bottom + 1).max(0) as f64;
        if places > (ITEM_LIMIT * 4) as f64 {
            return Err(format!(
                "That would be more than {ITEM_LIMIT} items: a larger spacing."
            ));
        }
        let mut offsets = Vec::new();
        for row in bottom..=top {
            for column in left..=right {
                if row == 0 && column == 0 {
                    continue;
                }
                let by = Vec2::new(column as f64 * across, row as f64 * up);
                let corners = [
                    bounds.min(),
                    Point2::new(bounds.max().x, bounds.min().y),
                    bounds.max(),
                    Point2::new(bounds.min().x, bounds.max().y),
                    bounds.center(),
                ];
                if corners.iter().all(|corner| self.inside(*corner + by)) {
                    offsets.push(by);
                }
            }
        }
        if offsets.len() > ITEM_LIMIT {
            return Err(format!(
                "That would be more than {ITEM_LIMIT} items: a larger spacing."
            ));
        }
        Ok(offsets)
    }

    fn commit(&self, up: f64, ctx: &mut CommandContext<'_>) -> Step {
        let Some(bounds) = bounds_of(ctx, &self.sources) else {
            return Step::Finished;
        };
        let offsets = match self.offsets(bounds, self.column_spacing, up) {
            Ok(offsets) => offsets,
            Err(why) => {
                ctx.message(why);
                return Step::Continue;
            }
        };
        if offsets.is_empty() {
            ctx.message("No copy fits inside the boundary at that spacing.");
            ctx.selection.clear();
            return Step::Finished;
        }
        let copies: Vec<(EntityId, Entity)> = offsets
            .iter()
            .flat_map(|by| {
                let transform = Transform::translation(*by);
                self.sources
                    .iter()
                    .filter_map(|id| Some((*id, ctx.drawing.entity(*id)?.transformed(&transform))))
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut transaction = ctx.transaction();
        for (source, entity) in copies {
            transaction
                .insert_like(source, entity)
                .expect("the selection is live");
        }
        transaction.commit();
        ctx.selection.clear();
        ctx.message(format!(
            "{} copies made inside the boundary.",
            offsets.len()
        ));
        Step::Finished
    }
}

impl Command for ArrayGridCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.adopt(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            GridState::Selecting => Prompt::new(SELECT_OBJECTS),
            GridState::Inside => Prompt::new("Pick an internal point of the boundary"),
            GridState::ColumnSpacing => Prompt::new("Specify the distance between columns"),
            GridState::RowSpacing => Prompt::new("Specify the distance between rows")
                .with_default(super::ltscale::display(self.column_spacing)),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            GridState::Selecting => selection_expected(),
            GridState::Inside => ExpectedInput::point(),
            GridState::ColumnSpacing | GridState::RowSpacing => ExpectedInput::distance(),
        }
    }

    fn takes_object_snap(&self) -> bool {
        !matches!(self.state, GridState::Inside)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (GridState::Selecting, CommandInput::Accept) => self.adopt(ctx),
            (GridState::RowSpacing, CommandInput::Accept) => self.commit(self.column_spacing, ctx),
            (_, CommandInput::Accept) => Step::Finished,
            (GridState::Inside, CommandInput::Point(at)) => {
                let candidates = candidates_without(ctx, &self.sources);
                match region_at(&candidates, at) {
                    Some(loops) => {
                        self.rings = rings_of(&loops);
                        self.state = GridState::ColumnSpacing;
                    }
                    None => ctx.message("No closed boundary round that point."),
                }
                Step::Continue
            }
            (GridState::ColumnSpacing | GridState::RowSpacing, CommandInput::Distance(spacing)) => {
                if !(spacing.is_finite() && spacing.abs() > 0.0) {
                    ctx.message("The spacing must be greater than zero.");
                    return Step::Continue;
                }
                if matches!(self.state, GridState::ColumnSpacing) {
                    self.column_spacing = spacing.abs();
                    self.state = GridState::RowSpacing;
                    Step::Continue
                } else {
                    self.commit(spacing.abs(), ctx)
                }
            }
            _ => Step::Continue,
        }
    }
}
