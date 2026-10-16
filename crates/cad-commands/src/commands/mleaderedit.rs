//! MLEADEREDIT (#241, ADR-0033): leaders added to a multileader, or taken
//! from it.
//!
//! A multileader is picked; then each point given is the arrowhead of a
//! new leader to its landing, previewed at the pointer, until Enter.
//! `Remove leader` takes the leader nearest each point given instead, and
//! `Add leader` goes back. A multileader keeps its last leader. Every
//! change replaces the multileader; the invocation is one undo.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, MLeaderEntity};
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const ADD: Keyword = Keyword::new("Add leader");
const REMOVE: Keyword = Keyword::new("Remove leader");

#[derive(Debug)]
enum State {
    Pick,
    Add(EntityId, MLeaderEntity),
    Remove(EntityId, MLeaderEntity),
}

#[derive(Debug)]
pub(crate) struct MLeaderEditCommand {
    state: State,
    /// The multileader being edited, while there is one: never a snap
    /// target while a leader is previewed on it (DESIGN §18.2).
    editing: Vec<EntityId>,
}

impl MLeaderEditCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Pick,
            editing: Vec::new(),
        }
    }

    /// Replaces the multileader with `edited`, and says so.
    fn replace(ctx: &mut CommandContext<'_>, id: EntityId, edited: &MLeaderEntity, done: &str) {
        let mut transaction = ctx.transaction();
        match transaction.replace(id, edited.clone()) {
            Ok(_) => {
                transaction.commit();
                ctx.message(done);
            }
            Err(error) => {
                drop(transaction);
                ctx.message(error.to_string());
            }
        }
    }
}

impl Command for MLeaderEditCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::Pick => Prompt::new("Select a multileader"),
            State::Add(..) => Prompt::new("Specify leader arrowhead location"),
            State::Remove(..) => Prompt::new("Specify leader to remove"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Pick => ExpectedInput::object(),
            State::Add(..) => ExpectedInput::point().with_keywords(&[REMOVE]),
            State::Remove(..) => ExpectedInput::point().with_keywords(&[ADD]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Pick);
        self.state = match (state, input) {
            (State::Pick, CommandInput::Pick { entity, .. }) => match ctx.drawing.entity(entity) {
                Some(Entity::MLeader(mleader)) => {
                    self.editing = vec![entity];
                    State::Add(entity, mleader.clone())
                }
                Some(other) => {
                    ctx.message(format!("A {} is not a multileader.", other.kind_name()));
                    State::Pick
                }
                None => State::Pick,
            },
            (_, CommandInput::Accept) => return Step::Finished,
            (State::Add(id, mleader), CommandInput::Point(tip)) => {
                let edited = mleader.with_leader(vec![tip]);
                Self::replace(ctx, id, &edited, "A leader is added.");
                State::Add(id, edited)
            }
            (State::Add(id, mleader), CommandInput::Keyword(REMOVE)) => State::Remove(id, mleader),
            (State::Remove(id, mleader), CommandInput::Point(at)) => {
                match mleader.without_leader(mleader.nearest_leader(at)) {
                    Ok(edited) => {
                        Self::replace(ctx, id, &edited, "A leader is removed.");
                        State::Remove(id, edited)
                    }
                    Err(_) => {
                        ctx.message("A multileader keeps at least one leader.");
                        State::Remove(id, mleader)
                    }
                }
            }
            (State::Remove(id, mleader), CommandInput::Keyword(ADD)) => State::Add(id, mleader),
            (state, _) => state,
        };
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Add(_, mleader) = &self.state {
            out.push(OverlayItem::PreviewEntity(
                mleader.with_leader(vec![cursor]).into(),
            ));
        }
    }

    fn transient_sources(&self) -> &[EntityId] {
        match self.state {
            State::Add(..) => &self.editing,
            _ => &[],
        }
    }
}
