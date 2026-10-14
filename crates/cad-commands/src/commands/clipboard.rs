//! The clipboard (#223): COPYCLIP, CUTCLIP, COPYBASE, and PASTECLIP, and
//! Ctrl+C, Ctrl+X, and Ctrl+V.
//!
//! The clipboard is the session's, not the system's (§45): a small drawing
//! of its own holding the objects copied, about their base point, and every
//! row they refer to (`TransactionBuilder::bring_in`). So it outlives the
//! drawing it came from, and a paste after `OPEN` brings the rows along,
//! matched by name, as a paste within the drawing does. COPYCLIP and
//! CUTCLIP take the lower left of what is copied as the base; COPYBASE
//! asks for it. A cut is one undo, as is a paste.

use cad_core::{Aabb, EntityId, Point2, Vec2};
use cad_document::{Drawing, EntityRecord};
use cad_input::ExpectedInput;

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

/// What was copied: the objects about their base point, in a drawing of
/// their own that holds the rows they refer to, and how far they reach.
#[derive(Debug)]
pub(crate) struct Clipboard {
    drawing: Drawing,
    records: Vec<EntityRecord>,
    extent: Aabb,
}

/// How a copy is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Taking {
    Copy,
    Cut,
    /// A copy about a base point asked for.
    WithBase,
}

#[derive(Debug, PartialEq, Eq)]
enum CopyState {
    Select,
    Base,
}

/// COPYCLIP, CUTCLIP, COPYBASE.
#[derive(Debug)]
pub(crate) struct CopyClipCommand {
    taking: Taking,
    state: CopyState,
}

impl CopyClipCommand {
    pub(crate) fn copy() -> Self {
        Self::taking(Taking::Copy)
    }

    pub(crate) fn cut() -> Self {
        Self::taking(Taking::Cut)
    }

    pub(crate) fn with_base() -> Self {
        Self::taking(Taking::WithBase)
    }

    fn taking(taking: Taking) -> Self {
        Self {
            taking,
            state: CopyState::Select,
        }
    }

    /// With the objects chosen: a base point next, or the copy now.
    fn chosen(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if self.taking == Taking::WithBase {
            self.state = CopyState::Base;
            return Step::Continue;
        }
        Self::take(ctx, self.taking, None)
    }

    /// Copies the selection to the clipboard about `base` — or the lower
    /// left of what it covers — and removes it for a cut.
    fn take(ctx: &mut CommandContext<'_>, taking: Taking, base: Option<Point2>) -> Step {
        let ids: Vec<EntityId> = ctx.selection.iter().collect();
        if ids.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }
        let records: Vec<EntityRecord> = ids
            .iter()
            .filter_map(|id| ctx.drawing.record(*id).cloned())
            .collect();
        let extent = records
            .iter()
            .map(|record| ctx.drawing.bounds_of(&record.entity))
            .reduce(|a, b| a.union(&b))
            .expect("some objects");
        let base = base.unwrap_or_else(|| extent.min());
        let offset: Vec2 = Point2::ORIGIN - base;
        let about: Vec<EntityRecord> = records
            .iter()
            .map(|record| EntityRecord::new(record.entity.translated(offset), record.properties))
            .collect();
        let mut drawing = Drawing::new();
        let mut transaction = drawing.transaction();
        let brought = match transaction.bring_in(ctx.drawing, &about) {
            Ok(brought) => brought,
            Err(error) => {
                drop(transaction);
                ctx.message(format!("The objects cannot be copied: {error}."));
                return Step::Finished;
            }
        };
        transaction.commit();
        let extent = Aabb::from_points(extent.min() + offset, extent.max() + offset);
        *ctx.clipboard = Some(Clipboard {
            drawing,
            records: brought,
            extent,
        });
        let count = counted(ids.len(), "object");
        if taking == Taking::Cut {
            let mut transaction = ctx.transaction();
            for id in &ids {
                transaction
                    .remove(*id)
                    .expect("selected entities are live and appear once");
            }
            transaction.commit();
            ctx.message(format!("{count} cut to the clipboard."));
        } else {
            ctx.message(format!("{count} copied to the clipboard."));
        }
        ctx.selection.clear();
        Step::Finished
    }
}

impl Command for CopyClipCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.chosen(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            CopyState::Select => Prompt::new(SELECT_OBJECTS),
            CopyState::Base => Prompt::new("Specify base point"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            CopyState::Select => selection_expected(),
            CopyState::Base => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (CopyState::Select, CommandInput::Accept) => self.chosen(ctx),
            (CopyState::Base, CommandInput::Point(base)) => {
                Self::take(ctx, self.taking, Some(base))
            }
            (CopyState::Base, CommandInput::Accept) => Step::Finished,
            _ => Step::Continue,
        }
    }
}

/// PASTECLIP: what the clipboard holds, placed by its base point.
#[derive(Debug)]
pub(crate) struct PasteClipCommand {
    /// What the clipboard covers, about its base point, for the preview.
    extent: Option<Aabb>,
}

impl PasteClipCommand {
    pub(crate) fn new() -> Self {
        Self { extent: None }
    }

    fn paste(ctx: &mut CommandContext<'_>, at: Point2) -> Step {
        let Some(clipboard) = ctx.clipboard.take() else {
            return Step::Finished;
        };
        let offset = at - Point2::ORIGIN;
        let mut transaction = ctx.transaction();
        let pasted = transaction
            .bring_in(&clipboard.drawing, &clipboard.records)
            .and_then(|brought| {
                brought.iter().try_for_each(|record| {
                    transaction
                        .insert_with(record.entity.translated(offset), record.properties)
                        .map(|_| ())
                })
            });
        let count = clipboard.records.len();
        match pasted {
            Ok(()) => {
                transaction.commit();
                ctx.message(format!("{} pasted.", counted(count, "object")));
            }
            Err(error) => {
                drop(transaction);
                ctx.message(format!("The clipboard cannot be pasted: {error}."));
            }
        }
        *ctx.clipboard = Some(clipboard);
        Step::Finished
    }
}

impl Command for PasteClipCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match ctx.clipboard.as_ref() {
            Some(clipboard) => {
                self.extent = Some(clipboard.extent);
                Step::Continue
            }
            None => {
                ctx.message("The clipboard is empty.");
                Step::Finished
            }
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Specify insertion point")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Point(at) => Self::paste(ctx, at),
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }

    /// Where the objects would land: the box they cover, at the pointer.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let Some(extent) = self.extent else {
            return;
        };
        let offset = cursor - Point2::ORIGIN;
        let (low, high) = (extent.min() + offset, extent.max() + offset);
        let corners = [
            low,
            Point2::new(high.x, low.y),
            high,
            Point2::new(low.x, high.y),
        ];
        for (index, corner) in corners.iter().enumerate() {
            out.push(OverlayItem::PreviewSegment {
                start: *corner,
                end: corners[(index + 1) % corners.len()],
            });
        }
    }
}
