//! DRAWORDER and TEXTTOFRONT (#226): what is painted over what.
//!
//! `DRAWORDER` (`DR`) takes the selection `Above objects` or `Under
//! objects` picked as a reference, to the `Front`, or to the `Back`. What
//! moves keeps its own order; what the tiers decide is
//! `cad_document::draw_order::reordered` (ADR-0032). `TEXTTOFRONT` brings
//! every text, dimension, or leader of the drawing in front of the rest.
//! One transaction, one undo each.

use std::collections::HashSet;

use cad_core::EntityId;
use cad_document::Entity;
use cad_document::draw_order::{Placement, reordered};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const ABOVE: Keyword = Keyword::new("Above objects");
const UNDER: Keyword = Keyword::new("Under objects");
const FRONT: Keyword = Keyword::new("Front");
const BACK: Keyword = Keyword::new("Back");
const PLACEMENTS: [Keyword; 4] = [ABOVE, UNDER, FRONT, BACK];

/// Puts `chosen` at `placement` in one transaction, and says what moved.
fn reorder(
    ctx: &mut CommandContext<'_>,
    chosen: &HashSet<EntityId>,
    placement: Placement,
) -> usize {
    let all: Vec<(EntityId, i16)> = ctx
        .drawing
        .records()
        .map(|(id, record)| (id, record.order))
        .collect();
    let moved = reordered(&all, chosen, placement);
    if moved.is_empty() {
        return 0;
    }
    let mut transaction = ctx.transaction();
    for (id, order) in &moved {
        transaction
            .set_order(*id, *order)
            .expect("a live entity, reordered once");
    }
    transaction.commit();
    moved.iter().filter(|(id, _)| chosen.contains(id)).count()
}

#[derive(Debug)]
enum State {
    Select,
    Placement(HashSet<EntityId>),
    /// The reference objects, for `Above` (true) or `Under`.
    Reference(HashSet<EntityId>, bool),
}

#[derive(Debug)]
pub(crate) struct DrawOrderCommand {
    state: State,
}

impl DrawOrderCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Select,
        }
    }

    fn chosen(ctx: &mut CommandContext<'_>) -> State {
        let chosen: HashSet<EntityId> = ctx.selection.iter().collect();
        if chosen.is_empty() {
            return State::Select;
        }
        ctx.selection.clear();
        State::Placement(chosen)
    }

    fn finish(ctx: &mut CommandContext<'_>, chosen: &HashSet<EntityId>, placement: Placement) {
        let moved = reorder(ctx, chosen, placement);
        let message = match (moved, placement) {
            (0, Placement::Front) => "The objects are already in front.".to_owned(),
            (0, Placement::Back) => "The objects are already at the back.".to_owned(),
            (0, _) => "The objects are already there.".to_owned(),
            (n, Placement::Front) => format!("{} brought to the front.", counted(n, "object")),
            (n, Placement::Back) => format!("{} sent to the back.", counted(n, "object")),
            (n, Placement::Above(_)) => format!("{} brought above.", counted(n, "object")),
            (n, Placement::Under(_)) => format!("{} sent under.", counted(n, "object")),
        };
        ctx.message(message);
    }
}

impl Command for DrawOrderCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if !ctx.selection.is_empty() {
            self.state = Self::chosen(ctx);
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Select => Prompt::new(SELECT_OBJECTS),
            State::Placement(_) => Prompt::new("Enter object ordering option").with_default("Back"),
            State::Reference(..) => Prompt::new("Select reference objects"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Select | State::Reference(..) => selection_expected(),
            State::Placement(_) => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&PLACEMENTS)
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Select);
        self.state = match (state, input) {
            (State::Select, CommandInput::Accept) => match Self::chosen(ctx) {
                State::Select => {
                    ctx.message(NOTHING_SELECTED);
                    return Step::Finished;
                }
                next => next,
            },
            (State::Placement(chosen), CommandInput::Keyword(FRONT)) => {
                Self::finish(ctx, &chosen, Placement::Front);
                return Step::Finished;
            }
            (State::Placement(chosen), CommandInput::Keyword(BACK) | CommandInput::Accept) => {
                Self::finish(ctx, &chosen, Placement::Back);
                return Step::Finished;
            }
            (State::Placement(chosen), CommandInput::Keyword(keyword)) => {
                State::Reference(chosen, keyword == ABOVE)
            }
            (State::Reference(chosen, above), CommandInput::Accept) => {
                let tiers: Vec<i16> = ctx
                    .selection
                    .iter()
                    .filter(|id| !chosen.contains(id))
                    .filter_map(|id| ctx.drawing.record(id).map(|record| record.order))
                    .collect();
                ctx.selection.clear();
                let reference = if above {
                    tiers.iter().max().map(|tier| Placement::Above(*tier))
                } else {
                    tiers.iter().min().map(|tier| Placement::Under(*tier))
                };
                match reference {
                    Some(placement) => Self::finish(ctx, &chosen, placement),
                    None => ctx.message("No reference objects selected."),
                }
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }
}

const TEXT: Keyword = Keyword::new("Text");
const DIMENSIONS: Keyword = Keyword::new("Dimensions");
const LEADERS: Keyword = Keyword::new("Leaders");
const ALL: Keyword = Keyword::new("All");
const ANNOTATIONS: [Keyword; 4] = [TEXT, DIMENSIONS, LEADERS, ALL];

/// TEXTTOFRONT: every text, dimension, or leader of the drawing in front.
#[derive(Debug)]
pub(crate) struct TextToFrontCommand;

impl Command for TextToFrontCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new("Bring to front").with_default("All")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing).with_keywords(&ANNOTATIONS)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let which = match input {
            CommandInput::Keyword(keyword) => keyword,
            CommandInput::Accept => ALL,
            _ => return Step::Continue,
        };
        let takes = |entity: &Entity| match entity {
            Entity::Text(_) | Entity::MText(_) | Entity::AttributeDefinition(_) => {
                which == TEXT || which == ALL
            }
            Entity::Dimension(_) => which == DIMENSIONS || which == ALL,
            Entity::Leader(_) => which == LEADERS || which == ALL,
            _ => false,
        };
        let chosen: HashSet<EntityId> = ctx
            .drawing
            .entities()
            .filter(|(_, entity)| takes(entity))
            .map(|(id, _)| id)
            .collect();
        let moved = reorder(ctx, &chosen, Placement::Front);
        ctx.message(match moved {
            0 if chosen.is_empty() => {
                "There is nothing of that kind to bring to the front.".to_owned()
            }
            0 => "They are already in front.".to_owned(),
            n => format!("{} brought to the front.", counted(n, "object")),
        });
        Step::Finished
    }
}
