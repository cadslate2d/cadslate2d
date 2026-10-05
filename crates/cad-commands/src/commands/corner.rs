//! FILLET and CHAMFER (DESIGN §22): pick two objects; they are cut back or
//! carried on to meet, joined by a tangent arc or by a straight bevel.
//! FILLET joins lines, arcs, and circles; CHAMFER joins two lines.
//!
//! One state machine serves both. They differ in the setting they keep —
//! one radius, or two distances — and in what joins the objects. The setting
//! is remembered for the session.

use cad_core::{EntityId, Point2};
use cad_document::{CornerError, Cornered, Entity};
use cad_geometry::CornerJoin;
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, DraftingSettings, OverlayItem, Prompt, Step};

const RADIUS: Keyword = Keyword::new("Radius");
const DISTANCE: Keyword = Keyword::new("Distance");
const POLYLINE: Keyword = Keyword::new("Polyline");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Fillet,
    Chamfer,
}

/// `5`, `2.5`: as short as the value allows.
fn display(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl Kind {
    /// FILLET joins lines, arcs, and circles; CHAMFER, lines only.
    fn first_prompt(self) -> &'static str {
        match self {
            Self::Fillet => "Select first object",
            Self::Chamfer => "Select first line",
        }
    }

    fn second_prompt(self) -> &'static str {
        match self {
            Self::Fillet => "Select second object",
            Self::Chamfer => "Select second line",
        }
    }

    /// The option that opens the setting.
    fn option(self) -> Keyword {
        match self {
            Self::Fillet => RADIUS,
            Self::Chamfer => DISTANCE,
        }
    }

    /// How many values the setting has, asked for one after the other.
    fn values(self) -> usize {
        match self {
            Self::Fillet => 1,
            Self::Chamfer => 2,
        }
    }

    fn value_prompt(self, index: usize) -> &'static str {
        match (self, index) {
            (Self::Fillet, _) => "Specify fillet radius",
            (Self::Chamfer, 0) => "Specify first chamfer distance",
            (Self::Chamfer, _) => "Specify second chamfer distance",
        }
    }

    fn value(self, settings: &DraftingSettings, index: usize) -> f64 {
        match self {
            Self::Fillet => settings.fillet_radius,
            Self::Chamfer => settings.chamfer_distances[index],
        }
    }

    fn set_value(self, settings: &mut DraftingSettings, index: usize, value: f64) {
        match self {
            Self::Fillet => settings.fillet_radius = value,
            // A new first distance is the natural default for the second.
            Self::Chamfer if index == 0 => settings.chamfer_distances = [value, value],
            Self::Chamfer => settings.chamfer_distances[1] = value,
        }
    }

    fn current(self, settings: &DraftingSettings) -> String {
        match self {
            Self::Fillet => format!("Current fillet radius: {}", display(settings.fillet_radius)),
            Self::Chamfer => {
                let [first, second] = settings.chamfer_distances;
                format!(
                    "Current chamfer distances: {}, {}",
                    display(first),
                    display(second)
                )
            }
        }
    }

    fn join(
        self,
        settings: &DraftingSettings,
        first: (&Entity, Point2),
        second: (&Entity, Point2),
    ) -> Result<Cornered, CornerError> {
        match self {
            Self::Fillet => first
                .0
                .filleted(first.1, second.0, second.1, settings.fillet_radius),
            Self::Chamfer => {
                first
                    .0
                    .chamfered(first.1, second.0, second.1, settings.chamfer_distances)
            }
        }
    }

    /// What this command puts at a corner of a polyline.
    fn corner_join(self, settings: &DraftingSettings) -> CornerJoin {
        match self {
            Self::Fillet => CornerJoin::Fillet(settings.fillet_radius),
            Self::Chamfer => CornerJoin::Chamfer(settings.chamfer_distances),
        }
    }

    /// `filleted` or `chamfered`, for messages.
    fn done(self) -> &'static str {
        match self {
            Self::Fillet => "filleted",
            Self::Chamfer => "chamfered",
        }
    }

    fn explain(self, error: CornerError) -> String {
        match (error, self) {
            // A polyline is joined only to itself.
            (CornerError::Unsupported("polyline"), _) => {
                "Pick two segments of the same polyline, or use the Polyline option.".to_owned()
            }
            (CornerError::Unsupported(kind), Self::Fillet) => {
                format!("FILLET does not support {kind}s yet.")
            }
            (CornerError::Unsupported(kind), Self::Chamfer) => {
                format!("CHAMFER joins two straight lines, not {kind}s.")
            }
            (CornerError::Parallel, _) => "The lines are parallel.".to_owned(),
            (CornerError::TooLarge, Self::Fillet) => {
                "The radius is too large for these lines.".to_owned()
            }
            (CornerError::TooLarge, Self::Chamfer) => {
                "The distances are too large for these lines.".to_owned()
            }
            (CornerError::NoSolution, _) => {
                "No fillet of that radius fits these objects.".to_owned()
            }
            (CornerError::NotAdjacent, _) => {
                "Pick two segments that meet at a corner of the polyline.".to_owned()
            }
        }
    }
}

#[derive(Debug)]
enum State {
    First,
    Second {
        first: EntityId,
        pick: Point2,
    },
    /// The Polyline option: waiting for the polyline whose every corner is done.
    WholePolyline,
    /// Entering value `index` of the setting; `current` is its default.
    Setting {
        index: usize,
        current: f64,
    },
}

#[derive(Debug)]
pub(crate) struct CornerCommand {
    kind: Kind,
    state: State,
}

impl CornerCommand {
    pub(crate) fn fillet() -> Self {
        Self::new(Kind::Fillet)
    }

    pub(crate) fn chamfer() -> Self {
        Self::new(Kind::Chamfer)
    }

    fn new(kind: Kind) -> Self {
        Self {
            kind,
            state: State::First,
        }
    }

    /// Moves on from setting value `index`: to the next value, or back to
    /// picking lines.
    fn after_value(&mut self, index: usize, ctx: &mut CommandContext<'_>) {
        let next = index + 1;
        self.state = if next < self.kind.values() {
            State::Setting {
                index: next,
                current: self.kind.value(ctx.settings, next),
            }
        } else {
            State::First
        };
    }

    /// Joins the two picked objects in one transaction. Two picks on one
    /// polyline round or bevel the corner between the picked segments.
    fn join(
        &self,
        first: (EntityId, Point2),
        second: (EntityId, Point2),
        ctx: &mut CommandContext<'_>,
    ) -> Result<(), String> {
        let (Some(a), Some(b)) = (ctx.drawing.entity(first.0), ctx.drawing.entity(second.0)) else {
            return Err("One of the objects no longer exists.".to_owned());
        };
        if first.0 == second.0 {
            if !matches!(a, Entity::Polyline(_)) {
                return Err("Pick a different object for the second one.".to_owned());
            }
            let cornered = a
                .with_corner_between(first.1, second.1, self.kind.corner_join(ctx.settings))
                .map_err(|error| self.kind.explain(error))?;
            let mut transaction = ctx.drawing.transaction();
            transaction
                .replace(first.0, cornered)
                .expect("the polyline was just read");
            transaction.commit();
            return Ok(());
        }
        let joined = self
            .kind
            .join(ctx.settings, (a, first.1), (b, second.1))
            .map_err(|error| self.kind.explain(error))?;

        let mut transaction = ctx.drawing.transaction();
        transaction
            .replace(first.0, joined.first)
            .expect("the first object was just read");
        transaction
            .replace(second.0, joined.second)
            .expect("the second object was just read");
        if let Some(joint) = joined.joint {
            transaction.insert(joint);
        }
        transaction.commit();
        Ok(())
    }

    /// The Polyline option: every corner of one polyline, in one transaction.
    fn every_corner(&self, id: EntityId, ctx: &mut CommandContext<'_>) -> Result<String, String> {
        let entity = ctx
            .drawing
            .entity(id)
            .ok_or_else(|| "That object no longer exists.".to_owned())?;
        let (cornered, done, skipped) = entity
            .with_every_corner(self.kind.corner_join(ctx.settings))
            .map_err(|_| "That is not a polyline.".to_owned())?;
        let corners = |count: usize| if count == 1 { "corner" } else { "corners" };
        let summary = format!(
            "{done} {} {}, {skipped} skipped.",
            corners(done),
            self.kind.done()
        );
        if done > 0 {
            let mut transaction = ctx.drawing.transaction();
            transaction
                .replace(id, cornered)
                .expect("the polyline was just read");
            transaction.commit();
        }
        Ok(summary)
    }
}

impl Command for CornerCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.message(self.kind.current(ctx.settings));
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::First => Prompt::new(self.kind.first_prompt()),
            State::Second { .. } => Prompt::new(self.kind.second_prompt()),
            State::WholePolyline => Prompt::new("Select polyline"),
            State::Setting { index, current } => {
                Prompt::new(self.kind.value_prompt(index)).with_default(display(current))
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::First => ExpectedInput::object().with_keywords(&[self.kind.option(), POLYLINE]),
            State::Second { .. } | State::WholePolyline => ExpectedInput::object(),
            State::Setting { .. } => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::First, CommandInput::Keyword(keyword)) if keyword == POLYLINE => {
                self.state = State::WholePolyline;
            }
            (State::WholePolyline, CommandInput::Pick { entity, .. }) => {
                match self.every_corner(entity, ctx) {
                    Ok(summary) => {
                        ctx.message(summary);
                        return Step::Finished;
                    }
                    Err(message) => ctx.message(message),
                }
            }
            (State::First, CommandInput::Keyword(_)) => {
                self.state = State::Setting {
                    index: 0,
                    current: self.kind.value(ctx.settings, 0),
                };
            }
            (State::Setting { index, .. }, CommandInput::Distance(value)) => {
                let index = *index;
                if value.is_finite() && value >= 0.0 {
                    self.kind.set_value(ctx.settings, index, value);
                    self.after_value(index, ctx);
                } else {
                    ctx.message("The value cannot be negative.");
                }
            }
            // Enter keeps the value shown.
            (State::Setting { index, .. }, CommandInput::Accept) => {
                let index = *index;
                self.after_value(index, ctx);
            }
            (State::First, CommandInput::Pick { entity, at }) => {
                self.state = State::Second {
                    first: entity,
                    pick: at,
                };
            }
            (State::Second { first, pick }, CommandInput::Pick { entity, at }) => {
                match self.join((*first, *pick), (entity, at), ctx) {
                    Ok(()) => return Step::Finished,
                    Err(message) => {
                        ctx.message(message);
                        self.state = State::First;
                    }
                }
            }
            (State::First | State::Second { .. } | State::WholePolyline, CommandInput::Accept) => {
                return Step::Finished;
            }
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, _cursor: Point2, out: &mut Vec<OverlayItem>) {
        // Keep the first line marked while the second is picked.
        if let State::Second { first, .. } = self.state {
            out.push(OverlayItem::Highlight(first));
        }
    }
}
