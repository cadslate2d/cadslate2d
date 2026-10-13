//! HATCH and HATCHEDIT (DESIGN §34; ADR-0021): fill a region, solid or
//! with a pattern, and change how a hatch is filled.
//!
//! The region is bounded by what is shown: a point inside it is picked,
//! and the smallest closed loop around that point is the boundary — a
//! closed polyline, a circle, or a chain of lines, arcs, and open
//! polylines that meet end to end — with the loops inside it as holes and
//! islands. Or the boundary objects are selected (`Select`), and every
//! loop they make is taken. `Pattern`, `Angle`, and `SCale` choose the
//! fill, which the session remembers for the next HATCH. The fill is
//! previewed at the pointer; one hatch is one transaction, on the current
//! layer. Lines that cross without meeting bound nothing yet (ADR-0021).
//!
//! HATCHEDIT picks a hatch and changes its pattern, angle, or scale, each
//! change replacing it, until Enter; the invocation is one undo step.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, HatchEntity, join};
use cad_geometry::{Path, flatten_path, ring_contains, signed_area};
use cad_input::{ExpectedInput, Keyword};

use super::hatch_look::{Look, LookOptions, OPTIONS};
use super::modify::selection_expected;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const SELECT: Keyword = Keyword::new("Select");
/// How finely arcs are followed when a loop is tested for the point.
const CHORD_STEP: f64 = std::f64::consts::TAU / 64.0;

#[derive(Debug)]
enum State {
    /// A point inside the region, or `Select`.
    Inside,
    /// The boundary objects.
    Selecting,
}

/// A loop the drawing offers as a boundary, with its ring for the tests.
#[derive(Clone, Debug)]
struct Candidate {
    path: Path,
    ring: Vec<Point2>,
}

impl Candidate {
    fn new(path: Path) -> Self {
        let ring = flatten_path(&path, CHORD_STEP);
        Self { path, ring }
    }

    fn area(&self) -> f64 {
        signed_area(&self.ring).abs()
    }

    fn contains(&self, point: Point2) -> bool {
        ring_contains(&self.ring, point)
    }
}

#[derive(Debug)]
pub(crate) struct HatchCommand {
    state: State,
    /// The loops the shown objects make, found when the command starts:
    /// what a picked point is looked up in, and what the preview uses.
    candidates: Vec<Candidate>,
    options: LookOptions,
}

const SELECT_AND_LOOK: [Keyword; 4] = [SELECT, OPTIONS[0], OPTIONS[1], OPTIONS[2]];

impl HatchCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Inside,
            candidates: Vec::new(),
            options: LookOptions::new(Look {
                pattern: 0,
                angle: 0.0,
                scale: 1.0,
                origin: Point2::ORIGIN,
            }),
        }
    }

    /// The hatch of `loops` as the options have it.
    fn hatch(&self, loops: Vec<Path>) -> HatchEntity {
        HatchEntity::solid(loops).styled(self.options.look.style())
    }

    /// The region a point inside it bounds: the smallest loop around the
    /// point, then every loop inside that one.
    fn region_at(&self, point: Point2) -> Option<Vec<Path>> {
        let outer = self
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.contains(point))
            .min_by(|(_, a), (_, b)| a.area().total_cmp(&b.area()))
            .map(|(index, _)| index)?;
        let boundary = &self.candidates[outer];
        let mut loops = vec![boundary.path.clone()];
        for (index, candidate) in self.candidates.iter().enumerate() {
            let inside = candidate
                .ring
                .first()
                .is_some_and(|probe| boundary.contains(*probe));
            if index != outer && inside && !candidate.contains(point) {
                loops.push(candidate.path.clone());
            }
        }
        Some(loops)
    }

    /// Commits a hatch of `loops` and finishes, or explains why not.
    fn commit(&self, loops: Vec<Path>, ctx: &mut CommandContext<'_>) -> Step {
        let hatch = self.hatch(loops);
        if let Err(error) = hatch.check() {
            ctx.message(error.explain());
            return Step::Continue;
        }
        let mut transaction = ctx.drawing.transaction();
        match transaction.insert(hatch) {
            Ok(_) => {
                transaction.commit();
                Step::Finished
            }
            Err(error) => {
                drop(transaction);
                ctx.message(error.to_string());
                Step::Continue
            }
        }
    }
}

/// The loops `entities` make: every closed polyline and circle as it is,
/// and every chain of the open curves that meets end to end.
pub(crate) fn loops_of(entities: &[&Entity]) -> Vec<Path> {
    let mut loops = Vec::new();
    let mut open: Vec<&Entity> = Vec::new();
    for entity in entities {
        match entity {
            Entity::Polyline(polyline) if polyline.closed => loops.push(polyline.path()),
            Entity::Circle(circle) => loops.push(Path {
                vertices: vec![
                    Point2::new(circle.center.x - circle.radius, circle.center.y),
                    Point2::new(circle.center.x + circle.radius, circle.center.y),
                ],
                bulges: vec![1.0, 1.0],
                closed: true,
            }),
            // A whole ellipse bounds a region as its chords (#181).
            Entity::Ellipse(ellipse) if ellipse.is_full() => {
                let mut vertices = ellipse.ellipse().chord_points();
                vertices.pop();
                loops.push(Path {
                    bulges: vec![0.0; vertices.len()],
                    vertices,
                    closed: true,
                });
            }
            Entity::Line(_) | Entity::Arc(_) | Entity::Polyline(_) => open.push(entity),
            _ => {}
        }
    }
    if open.len() >= 2
        && let Ok(joined) = join(&open)
    {
        for result in joined {
            if let Entity::Polyline(polyline) = result.entity
                && polyline.closed
            {
                loops.push(polyline.path());
            }
        }
    }
    loops
}

impl Command for HatchCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let shown: Vec<&Entity> = ctx.drawing.shown().map(|(_, entity)| entity).collect();
        self.candidates = loops_of(&shown).into_iter().map(Candidate::new).collect();
        self.options = LookOptions::new(Look::remembered(ctx));
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        if let Some(prompt) = self.options.prompt() {
            return prompt;
        }
        match self.state {
            State::Inside => Prompt::new("Pick internal point"),
            State::Selecting => Prompt::new("Select boundary objects"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        if let Some(expected) = self.options.expected() {
            return expected;
        }
        match self.state {
            State::Inside => ExpectedInput::point().with_keywords(&SELECT_AND_LOOK),
            State::Selecting => selection_expected(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        if let Some(changed) = self.options.step(&input, ctx) {
            if changed {
                self.options.look.remember(ctx);
            }
            return Step::Continue;
        }
        match (&self.state, input) {
            (State::Inside, CommandInput::Point(point)) => match self.region_at(point) {
                Some(loops) => return self.commit(loops, ctx),
                None => ctx.message("No closed boundary surrounds that point."),
            },
            (State::Inside, CommandInput::Keyword(SELECT)) => {
                ctx.selection.clear();
                self.state = State::Selecting;
            }
            (State::Inside, CommandInput::Keyword(keyword)) => {
                self.options.open(keyword);
            }
            (State::Selecting, CommandInput::Accept) => {
                let selected: Vec<&Entity> = ctx
                    .selection
                    .iter()
                    .filter_map(|id| ctx.drawing.entity(id))
                    .collect();
                if selected.is_empty() {
                    ctx.message("No objects selected.");
                    return Step::Finished;
                }
                let loops = loops_of(&selected);
                ctx.selection.clear();
                if loops.is_empty() {
                    ctx.message("The boundary does not close.");
                    return Step::Finished;
                }
                return self.commit(loops, ctx);
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Inside = self.state
            && let Some(loops) = self.region_at(cursor)
        {
            let hatch = self.hatch(loops);
            if hatch.check().is_ok() {
                out.push(OverlayItem::PreviewEntity(hatch.into()));
            }
        }
    }

    /// A point inside a region is a place, not a point of the drawing.
    fn takes_object_snap(&self) -> bool {
        false
    }
}

#[derive(Debug)]
enum EditState {
    Pick,
    /// The hatch picked: an option, or Enter.
    Options(EntityId),
}

/// HATCHEDIT.
#[derive(Debug)]
pub(crate) struct HatchEditCommand {
    state: EditState,
    options: LookOptions,
}

impl HatchEditCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: EditState::Pick,
            options: LookOptions::new(Look {
                pattern: 0,
                angle: 0.0,
                scale: 1.0,
                origin: Point2::ORIGIN,
            }),
        }
    }

    /// Fills the hatch the new way, or explains why it cannot be.
    fn restyle(&mut self, id: EntityId, before: Look, ctx: &mut CommandContext<'_>) {
        let Some(Entity::Hatch(hatch)) = ctx.drawing.entity(id) else {
            return;
        };
        let look = self.options.look;
        if look.is_solid() && look.pattern == before.pattern && look != before {
            ctx.message("A solid fill has no angle or scale: choose a pattern first.");
            self.options.look = before;
            return;
        }
        let restyled = hatch.styled(look.style());
        if let Err(error) = restyled.check() {
            ctx.message(error.explain());
            self.options.look = before;
            return;
        }
        let mut transaction = ctx.drawing.transaction();
        transaction
            .replace(id, restyled)
            .expect("the hatch was picked from the drawing");
        transaction.commit();
        ctx.message(format!("Hatch filled with {}.", look.name()));
    }
}

impl Command for HatchEditCommand {
    fn prompt(&self) -> Prompt {
        if let Some(prompt) = self.options.prompt() {
            return prompt;
        }
        match self.state {
            EditState::Pick => Prompt::new("Select hatch object"),
            EditState::Options(_) => Prompt::new("Press Enter to finish"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        if let Some(expected) = self.options.expected() {
            return expected;
        }
        match self.state {
            EditState::Pick => ExpectedInput::object(),
            EditState::Options(_) => {
                ExpectedInput::new(cad_input::InputKind::Nothing).with_keywords(&OPTIONS)
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        if let EditState::Options(id) = self.state {
            let before = self.options.look;
            match self.options.step(&input, ctx) {
                Some(true) => {
                    self.restyle(id, before, ctx);
                    return Step::Continue;
                }
                Some(false) => return Step::Continue,
                None => {}
            }
        }
        match (&self.state, input) {
            (EditState::Pick, CommandInput::Pick { entity, .. }) => {
                match ctx.drawing.entity(entity) {
                    Some(Entity::Hatch(hatch)) => {
                        self.options = LookOptions::new(Look::of(hatch));
                        self.state = EditState::Options(entity);
                    }
                    Some(_) => ctx.message("That is not a hatch."),
                    None => {}
                }
            }
            (EditState::Options(_), CommandInput::Keyword(keyword)) => {
                self.options.open(keyword);
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }
}
