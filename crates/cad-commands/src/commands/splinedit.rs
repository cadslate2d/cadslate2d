//! SPLINEDIT (#237): a spline closed or opened, reversed, its fit points
//! added or deleted, or turned into a polyline.
//!
//! A spline is picked, then options until Enter: `Close` or `Open`, `Fit
//! data` (`Add` a point after the fit point nearest the one given, or
//! `Delete` the nearest), `Reverse`, and `Polyline`, which ends the
//! command with the spline's chords as a polyline in its place. What each
//! does is `cad_document`'s; every change replaces the spline, and the
//! invocation is one undo.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, SplineEditError, SplineEntity};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const CLOSE: Keyword = Keyword::new("Close");
const OPEN: Keyword = Keyword::new("Open");
const FIT: Keyword = Keyword::new("Fit data");
const REVERSE: Keyword = Keyword::new("Reverse");
const POLYLINE: Keyword = Keyword::new("Polyline");
const ADD: Keyword = Keyword::new("Add");
const DELETE: Keyword = Keyword::new("Delete");

#[derive(Debug)]
enum State {
    Pick,
    Option(EntityId),
    Fit(EntityId),
    /// The fit point a new one goes after, or the one to delete.
    FitPoint(EntityId, Keyword),
    NewPoint(EntityId, usize),
}

#[derive(Debug)]
pub(crate) struct SplineEditCommand {
    state: State,
}

impl SplineEditCommand {
    pub(crate) fn new() -> Self {
        Self { state: State::Pick }
    }

    fn spline(ctx: &CommandContext<'_>, id: EntityId) -> Option<SplineEntity> {
        match ctx.drawing.entity(id) {
            Some(Entity::Spline(spline)) => Some(spline.clone()),
            _ => None,
        }
    }

    /// Replaces the spline with what `edit` makes of it, or says why not.
    fn edit(
        ctx: &mut CommandContext<'_>,
        id: EntityId,
        edit: impl FnOnce(&SplineEntity) -> Result<SplineEntity, SplineEditError>,
    ) {
        let Some(spline) = Self::spline(ctx, id) else {
            return;
        };
        match edit(&spline) {
            Ok(edited) => {
                let mut transaction = ctx.transaction();
                transaction
                    .replace(id, edited)
                    .expect("the spline picked, replaced once");
                transaction.commit();
            }
            Err(error) => ctx.message(error.to_string()),
        }
    }
}

impl Command for SplineEditCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::Pick => Prompt::new("Select spline"),
            State::Option(_) => Prompt::new("Enter an option").with_default("eXit"),
            State::Fit(_) => Prompt::new("Enter a fit data option").with_default("eXit"),
            State::FitPoint(_, ADD) => Prompt::new("Specify the fit point to add after"),
            State::FitPoint(..) => Prompt::new("Specify the fit point to delete"),
            State::NewPoint(..) => Prompt::new("Specify new fit point"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Pick => ExpectedInput::object(),
            State::Option(_) => {
                // Close for an open spline, Open for a closed one: the
                // command cannot ask the drawing here, so both are offered.
                ExpectedInput::new(InputKind::Nothing)
                    .with_keywords(&[CLOSE, OPEN, FIT, REVERSE, POLYLINE])
            }
            State::Fit(_) => ExpectedInput::new(InputKind::Nothing).with_keywords(&[ADD, DELETE]),
            State::FitPoint(..) | State::NewPoint(..) => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Pick);
        self.state = match (state, input) {
            (State::Pick, CommandInput::Pick { entity, .. }) => match ctx.drawing.entity(entity) {
                Some(Entity::Spline(_)) => State::Option(entity),
                Some(other) => {
                    ctx.message(format!("A {} is not a spline.", other.kind_name()));
                    State::Pick
                }
                None => State::Pick,
            },
            (State::Pick | State::Option(_), CommandInput::Accept) => return Step::Finished,
            (State::Option(id), CommandInput::Keyword(CLOSE)) => {
                Self::edit(ctx, id, SplineEntity::closed);
                State::Option(id)
            }
            (State::Option(id), CommandInput::Keyword(OPEN)) => {
                Self::edit(ctx, id, SplineEntity::opened);
                State::Option(id)
            }
            (State::Option(id), CommandInput::Keyword(REVERSE)) => {
                Self::edit(ctx, id, |spline| Ok(spline.reversed()));
                ctx.message("The spline is reversed.");
                State::Option(id)
            }
            (State::Option(id), CommandInput::Keyword(POLYLINE)) => {
                if let Some(spline) = Self::spline(ctx, id) {
                    let polyline = spline.to_polyline();
                    let count = polyline.vertices.len();
                    let mut transaction = ctx.transaction();
                    transaction
                        .replace(id, polyline)
                        .expect("the spline picked, replaced once");
                    transaction.commit();
                    ctx.message(format!("The spline is a polyline of {count} vertices."));
                }
                return Step::Finished;
            }
            (State::Option(id), CommandInput::Keyword(_)) => match Self::spline(ctx, id) {
                Some(spline) if spline.fit.is_empty() => {
                    ctx.message(SplineEditError::NoFitData.to_string());
                    State::Option(id)
                }
                _ => State::Fit(id),
            },
            (State::Fit(id), CommandInput::Keyword(option)) => State::FitPoint(id, option),
            (State::Fit(id), CommandInput::Accept) => State::Option(id),
            (State::FitPoint(id, option), CommandInput::Point(at)) => {
                let nearest = Self::spline(ctx, id).and_then(|spline| spline.nearest_fit_point(at));
                match (option, nearest) {
                    (ADD, Some(index)) => State::NewPoint(id, index),
                    (_, Some(index)) => {
                        Self::edit(ctx, id, |spline| spline.without_fit_point(index));
                        State::Fit(id)
                    }
                    (_, None) => State::Fit(id),
                }
            }
            (State::FitPoint(id, _), CommandInput::Accept) => State::Fit(id),
            (State::NewPoint(id, after), CommandInput::Point(point)) => {
                Self::edit(ctx, id, |spline| spline.with_fit_point(after, point));
                State::Fit(id)
            }
            (State::NewPoint(id, _), CommandInput::Accept) => State::Fit(id),
            (state, _) => state,
        };
        Step::Continue
    }

    fn base_point(&self) -> Option<Point2> {
        None
    }
}
