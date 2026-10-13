//! The grip edit (DESIGN §16.3): one grip of one selected entity, dragged
//! to a new position.
//!
//! It is a command like any other, started by clicking a grip instead of by
//! name. That is what gives a grip edit typed coordinates, object snaps,
//! ORTHO and POLAR, a transient preview, one transaction, and one undo step
//! without any machinery of its own.

use cad_core::{EntityId, Point2};
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, Grip, OverlayItem, Prompt, Step};

/// The name the session shows for an active grip edit.
pub(crate) const NAME: &str = "STRETCH";

#[derive(Debug)]
pub(crate) struct GripEditCommand {
    grip: Grip,
    /// The entity being edited: not a snap target while the edit is open.
    sources: [EntityId; 1],
}

impl GripEditCommand {
    pub(crate) fn new(grip: Grip) -> Self {
        Self {
            grip,
            sources: [grip.id.entity],
        }
    }
}

impl Command for GripEditCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.message(format!("** {NAME} **"));
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Specify stretch point")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point().with_direct_distance()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let to = match input {
            CommandInput::Point(to) => to,
            // Enter with the pointer off the canvas: nothing to commit.
            CommandInput::Accept => return Step::Finished,
            _ => return Step::Continue,
        };
        let id = self.grip.id;
        let Some(entity) = ctx.drawing.entity(id.entity) else {
            return Step::Finished;
        };
        match entity.grip_moved(id.index, to) {
            // Put back where it was: not an edit, and not an undo step.
            Some(edited) if &edited == entity => Step::Finished,
            Some(edited) => {
                let mut transaction = ctx.transaction();
                transaction
                    .replace(id.entity, edited)
                    .expect("the entity was just read");
                transaction.commit();
                Step::Finished
            }
            None => {
                ctx.message("The object cannot be stretched to that point.");
                Step::Continue
            }
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        out.push(OverlayItem::PreviewSegment {
            start: self.grip.position,
            end: cursor,
        });
        out.push(OverlayItem::PreviewGrip {
            id: self.grip.id,
            to: cursor,
        });
    }

    fn base_point(&self) -> Option<Point2> {
        Some(self.grip.position)
    }

    fn transient_sources(&self) -> &[EntityId] {
        &self.sources
    }

    fn grip(&self) -> Option<Grip> {
        Some(self.grip)
    }
}
