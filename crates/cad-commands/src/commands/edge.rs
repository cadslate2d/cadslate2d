//! TRIM and EXTEND (DESIGN §22): choose the edges, then pick objects — each
//! pick is cut back between the edges around it, or carried on to the first
//! edge it meets.
//!
//! The two commands are one state machine. They differ in what a pick does
//! and in their wording. Edges are chosen like any selection set; Enter with
//! nothing chosen makes every object an edge. What each kind of object
//! supports is decided by `Entity::trim_parts_at` and `Entity::extended_to`,
//! and published in `docs/modify-matrix.md`.

use cad_core::{Aabb, EntityId, Point2};
use cad_document::{Drawing, Entity, ExtendError, LayoutId, TrimError};
use cad_geometry::Primitive;
use cad_input::ExpectedInput;

use super::modify::selection_expected;
use crate::{Command, CommandContext, CommandInput, HoveredObject, OverlayItem, Prompt, Step};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Trim,
    Extend,
}

impl Operation {
    fn edges_prompt(self) -> &'static str {
        match self {
            Self::Trim => "Select cutting edges",
            Self::Extend => "Select boundary edges",
        }
    }

    fn target_prompt(self) -> &'static str {
        match self {
            Self::Trim => "Select object to trim",
            Self::Extend => "Select object to extend",
        }
    }
}

/// Which objects cut or bound.
#[derive(Debug)]
enum Edges {
    /// Every object in the drawing: Enter was pressed with nothing chosen.
    All,
    Chosen(Vec<EntityId>),
}

/// What a pick does: the entities that replace the picked one — the first
/// takes its place, any others are new — and, for a trim, the part removed.
#[derive(Debug)]
struct Edit {
    replacement: Vec<Entity>,
    removed: Option<Entity>,
}

#[derive(Debug)]
enum State {
    Edges,
    Target { edges: Edges },
}

#[derive(Debug)]
pub(crate) struct EdgeCommand {
    operation: Operation,
    state: State,
    /// The space the command runs in, from `start` (#191).
    space: LayoutId,
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
            state: State::Edges,
            space: LayoutId::MODEL,
        }
    }

    /// The edges that could matter to `target` within `region`, never the
    /// target itself: an object that is both an edge and a target is cut or
    /// bounded by the others.
    fn edges_near<'a>(
        edges: &Edges,
        drawing: &'a Drawing,
        space: LayoutId,
        target: EntityId,
        region: &Aabb,
    ) -> Vec<&'a Entity> {
        let ids = match edges {
            // Candidates come from the index, never from a scan of the drawing.
            // An object on a locked layer is an edge like any other; one on
            // a layer that is off is not there.
            Edges::All => drawing.query_shown_in(space, region),
            Edges::Chosen(ids) => ids.clone(),
        };
        ids.into_iter()
            .filter(|id| *id != target)
            .filter_map(|id| drawing.entity(id))
            .collect()
    }

    /// What picking `target` at `pick` would do. The hover preview and the
    /// committed edit both come from here, so they cannot differ.
    fn edit(
        &self,
        edges: &Edges,
        drawing: &Drawing,
        target: EntityId,
        pick: Point2,
    ) -> Result<Edit, String> {
        let shape = drawing
            .entity(target)
            .ok_or_else(|| "That object no longer exists.".to_owned())?;
        let space = self.space;
        match self.operation {
            Operation::Trim => {
                let cuts: Vec<Point2> =
                    Self::edges_near(edges, drawing, space, target, &drawing.bounds_of(shape))
                        .into_iter()
                        .flat_map(|edge| shape.intersections(edge, drawing.tables()))
                        .collect();
                let parts = shape
                    .trim_parts_at(&cuts, pick)
                    .map_err(|error| match error {
                        TrimError::Unsupported(kind) => {
                            format!("TRIM does not support {kind}s yet.")
                        }
                        TrimError::NoIntersection => {
                            "The object does not cross a cutting edge.".to_owned()
                        }
                        TrimError::NeedsTwoCrossings(kind) => {
                            format!(
                                "A {kind} must be crossed twice by cutting edges to be trimmed."
                            )
                        }
                    })?;
                Ok(Edit {
                    replacement: parts.kept,
                    removed: Some(parts.removed),
                })
            }
            Operation::Extend => {
                let within = drawing
                    .extents_in(space)
                    .unwrap_or_else(|| drawing.bounds_of(shape));
                let boundary: Vec<Primitive> = shape
                    .extension_reach(pick, &within)
                    .map(|reach| Self::edges_near(edges, drawing, space, target, &reach))
                    .unwrap_or_default()
                    .into_iter()
                    .flat_map(|edge| edge.primitives(drawing.tables()))
                    .collect();
                let longer = shape
                    .extended_to(&boundary, pick)
                    .map_err(|error| match error {
                        ExtendError::Unsupported(kind) => {
                            format!("EXTEND does not support {kind}s yet.")
                        }
                        ExtendError::NoMeeting => {
                            "The object does not meet a boundary edge when extended.".to_owned()
                        }
                    })?;
                Ok(Edit {
                    replacement: vec![longer],
                    removed: None,
                })
            }
        }
    }

    /// One pick: one transaction, or a message and no change.
    fn commit(&self, edges: &Edges, target: EntityId, pick: Point2, ctx: &mut CommandContext<'_>) {
        match self.edit(edges, ctx.drawing, target, pick) {
            Ok(edit) => {
                let mut pieces = edit.replacement.into_iter();
                let mut transaction = ctx.transaction();
                if let Some(first) = pieces.next() {
                    transaction
                        .replace(target, first)
                        .expect("the target was just read");
                }
                for piece in pieces {
                    transaction
                        .insert_like(target, piece)
                        .expect("the target was just read");
                }
                transaction.commit();
            }
            Err(message) => ctx.message(message),
        }
    }
}

impl Command for EdgeCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        // The edges are chosen here, from nothing: an earlier selection is
        // not silently taken to be them.
        ctx.selection.clear();
        self.space = ctx.space;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Edges => Prompt::new(self.operation.edges_prompt()).with_default("all"),
            State::Target { .. } => Prompt::new(self.operation.target_prompt()),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Edges => selection_expected(),
            State::Target { .. } => ExpectedInput::object(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Edges, CommandInput::Accept) => {
                let chosen: Vec<EntityId> = ctx.selection.iter().collect();
                ctx.selection.clear();
                let edges = if chosen.is_empty() {
                    ctx.message("Every object is an edge.");
                    Edges::All
                } else {
                    Edges::Chosen(chosen)
                };
                self.state = State::Target { edges };
            }
            (State::Target { edges }, CommandInput::Pick { entity, at }) => {
                self.commit(edges, entity, at, ctx);
            }
            (State::Target { .. }, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// What a click on the hovered object would do: the part TRIM would cut
    /// away, or the object as EXTEND would lengthen it.
    fn preview_pick(&self, hovered: &HoveredObject<'_>, out: &mut Vec<OverlayItem>) {
        let State::Target { edges } = &self.state else {
            return;
        };
        let Ok(edit) = self.edit(edges, hovered.drawing, hovered.id, hovered.at) else {
            return;
        };
        match edit.removed {
            Some(removed) => out.push(OverlayItem::PreviewRemoval(removed)),
            None => out.extend(edit.replacement.into_iter().map(OverlayItem::PreviewEntity)),
        }
    }

    fn preview(&self, _cursor: Point2, out: &mut Vec<OverlayItem>) {
        // Keep the chosen edges marked while objects are picked against them.
        if let State::Target {
            edges: Edges::Chosen(ids),
        } = &self.state
        {
            out.extend(ids.iter().copied().map(OverlayItem::Highlight));
        }
    }
}
