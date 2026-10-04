//! FILLET and CHAMFER for two lines (DESIGN §22): pick two lines; they are
//! cut back or carried on to a corner, joined by a tangent arc or by a
//! straight bevel.
//!
//! One state machine serves both. They differ in the setting they keep —
//! one radius, or two distances — and in what joins the lines. The setting
//! is remembered for the session.

use cad_core::{EntityId, Point2};
use cad_document::{CornerError, Cornered, Entity};
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, DraftingSettings, OverlayItem, Prompt, Step};

const RADIUS: Keyword = Keyword::new("Radius");
const DISTANCE: Keyword = Keyword::new("Distance");

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
    fn name(self) -> &'static str {
        match self {
            Self::Fillet => "FILLET",
            Self::Chamfer => "CHAMFER",
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

    fn explain(self, error: CornerError) -> String {
        match (error, self) {
            (CornerError::Unsupported(kind), _) => {
                format!(
                    "{} joins two lines; {kind}s are not supported yet.",
                    self.name()
                )
            }
            (CornerError::Parallel, _) => "The lines are parallel.".to_owned(),
            (CornerError::TooLarge, Self::Fillet) => {
                "The radius is too large for these lines.".to_owned()
            }
            (CornerError::TooLarge, Self::Chamfer) => {
                "The distances are too large for these lines.".to_owned()
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

    /// Joins the two picked lines in one transaction.
    fn join(
        &self,
        first: (EntityId, Point2),
        second: (EntityId, Point2),
        ctx: &mut CommandContext<'_>,
    ) -> Result<(), String> {
        if first.0 == second.0 {
            return Err("Pick a different object for the second one.".to_owned());
        }
        let (Some(a), Some(b)) = (ctx.drawing.entity(first.0), ctx.drawing.entity(second.0)) else {
            return Err("One of the objects no longer exists.".to_owned());
        };
        let joined = self
            .kind
            .join(ctx.settings, (a, first.1), (b, second.1))
            .map_err(|error| self.kind.explain(error))?;

        let mut transaction = ctx.drawing.transaction();
        transaction
            .replace(first.0, joined.first)
            .expect("the first line was just read");
        transaction
            .replace(second.0, joined.second)
            .expect("the second line was just read");
        if let Some(joint) = joined.joint {
            transaction.insert(joint);
        }
        transaction.commit();
        Ok(())
    }
}

impl Command for CornerCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.message(self.kind.current(ctx.settings));
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::First => Prompt::new("Select first line"),
            State::Second { .. } => Prompt::new("Select second line"),
            State::Setting { index, current } => {
                Prompt::new(self.kind.value_prompt(index)).with_default(display(current))
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::First => ExpectedInput::object().with_keywords(&[self.kind.option()]),
            State::Second { .. } => ExpectedInput::object(),
            State::Setting { .. } => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
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
            (State::First | State::Second { .. }, CommandInput::Accept) => return Step::Finished,
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
