//! TRIM and EXTEND against one edge (DESIGN §22): choose the edge, then
//! pick objects — each pick is cut back to the edge, or carried on to it.
//!
//! The two commands are one state machine. They differ in what a pick does
//! and in their wording. What each kind of object supports is decided by
//! `Entity::trimmed` and `Entity::extended`, and published in
//! `docs/modify-matrix.md`.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, ExtendError, TrimError};
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, HoveredObject, OverlayItem, Prompt, Step};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Trim,
    Extend,
}

impl Operation {
    fn edge_prompt(self) -> &'static str {
        match self {
            Self::Trim => "Select cutting edge",
            Self::Extend => "Select boundary edge",
        }
    }

    fn target_prompt(self) -> &'static str {
        match self {
            Self::Trim => "Select object to trim",
            Self::Extend => "Select object to extend",
        }
    }

    fn itself(self) -> &'static str {
        match self {
            Self::Trim => "The cutting edge cannot trim itself.",
            Self::Extend => "The boundary edge cannot be extended to itself.",
        }
    }

    /// What `target` becomes when picked at `pick`: the entities that replace
    /// it — the first takes its place, any others are new — or why not.
    fn apply(self, target: &Entity, edge: &Entity, pick: Point2) -> Result<Vec<Entity>, String> {
        match self {
            Self::Trim => target.trimmed(edge, pick).map_err(|error| match error {
                TrimError::Unsupported(kind) => format!("TRIM does not support {kind}s yet."),
                TrimError::NoIntersection => {
                    "The object does not cross the cutting edge.".to_owned()
                }
                TrimError::CircleNeedsTwo => {
                    "A circle must cross the cutting edge twice to be trimmed.".to_owned()
                }
            }),
            Self::Extend => target
                .extended(edge, pick)
                .map(|longer| vec![longer])
                .map_err(|error| match error {
                    ExtendError::Unsupported(kind) => {
                        format!("EXTEND does not support {kind}s yet.")
                    }
                    ExtendError::NoMeeting => {
                        "The object does not meet the boundary edge when extended.".to_owned()
                    }
                }),
        }
    }
}

#[derive(Debug)]
enum State {
    Edge,
    Target { edge: EntityId },
}

#[derive(Debug)]
pub(crate) struct EdgeCommand {
    operation: Operation,
    state: State,
}

impl EdgeCommand {
    pub(crate) fn trimming() -> Self {
        Self::new(Operation::Trim)
    }

    pub(crate) fn extending() -> Self {
        Self::new(Operation::Extend)
    }

    fn new(operation: Operation) -> Self {
        Self {
            operation,
            state: State::Edge,
        }
    }

    /// One pick: one transaction, or a message and no change.
    fn edit(&self, edge: EntityId, target: EntityId, pick: Point2, ctx: &mut CommandContext<'_>) {
        if target == edge {
            ctx.message(self.operation.itself());
            return;
        }
        let (Some(edge), Some(shape)) = (ctx.drawing.entity(edge), ctx.drawing.entity(target))
        else {
            return;
        };
        match self.operation.apply(shape, edge, pick) {
            Ok(pieces) => {
                let mut pieces = pieces.into_iter();
                let mut transaction = ctx.drawing.transaction();
                if let Some(first) = pieces.next() {
                    transaction
                        .replace(target, first)
                        .expect("the target was just read");
                }
                for piece in pieces {
                    transaction.insert(piece);
                }
                transaction.commit();
            }
            Err(message) => ctx.message(message),
        }
    }
}

impl Command for EdgeCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match self.state {
            State::Edge => self.operation.edge_prompt(),
            State::Target { .. } => self.operation.target_prompt(),
        })
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::object()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Edge, CommandInput::Pick { entity, .. }) => {
                self.state = State::Target { edge: entity };
            }
            (State::Target { edge }, CommandInput::Pick { entity, at }) => {
                self.edit(*edge, entity, at, ctx);
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// What a click on the hovered object would do: the part TRIM would cut
    /// away, or the object as EXTEND would lengthen it.
    fn preview_pick(&self, hovered: &HoveredObject<'_>, out: &mut Vec<OverlayItem>) {
        let State::Target { edge } = self.state else {
            return;
        };
        let Some(edge) = hovered.drawing.entity(edge).filter(|_| hovered.id != edge) else {
            return;
        };
        match self.operation {
            Operation::Trim => {
                if let Ok(parts) = hovered.entity.trim_parts(edge, hovered.at) {
                    out.push(OverlayItem::PreviewRemoval(parts.removed));
                }
            }
            Operation::Extend => {
                if let Ok(longer) = hovered.entity.extended(edge, hovered.at) {
                    out.push(OverlayItem::PreviewEntity(longer));
                }
            }
        }
    }

    fn preview(&self, _cursor: Point2, out: &mut Vec<OverlayItem>) {
        // Keep the chosen edge marked while objects are picked against it.
        if let State::Target { edge } = self.state {
            out.push(OverlayItem::Highlight(edge));
        }
    }
}
