//! PEDIT (#204): a polyline edited as one — closed or opened, joined to
//! what meets it, given a width, its vertices walked and moved, its arcs
//! straightened — each option one transaction, the command one undo, and
//! `Undo` within it stepping back through its own changes. A line or an
//! arc picked is offered to become a polyline first.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, PolylineEntity, join};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");
const CLOSE: Keyword = Keyword::new("Close");
const OPEN: Keyword = Keyword::new("Open");
const JOIN: Keyword = Keyword::new("Join");
const WIDTH: Keyword = Keyword::new("Width");
const EDIT_VERTEX: Keyword = Keyword::new("Edit vertex");
const DECURVE: Keyword = Keyword::new("Decurve");
const UNDO: Keyword = Keyword::new("Undo");
const OPTIONS: [Keyword; 7] = [CLOSE, OPEN, JOIN, WIDTH, EDIT_VERTEX, DECURVE, UNDO];
const NEXT: Keyword = Keyword::new("Next");
const PREVIOUS: Keyword = Keyword::new("Previous");
const INSERT: Keyword = Keyword::new("Insert");
const MOVE: Keyword = Keyword::new("Move");
const STRAIGHTEN: Keyword = Keyword::new("Straighten");
const EXIT: Keyword = Keyword::new("Exit");
const VERTEX_OPTIONS: [Keyword; 6] = [NEXT, PREVIOUS, INSERT, MOVE, STRAIGHTEN, EXIT];

#[derive(Debug)]
enum State {
    Pick,
    /// A line or an arc picked: make it a polyline?
    Convert(EntityId),
    Options,
    JoinSelect,
    Width,
    Vertex {
        index: usize,
    },
    VertexInsert {
        index: usize,
    },
    VertexMove {
        index: usize,
    },
}

#[derive(Debug)]
pub(crate) struct PeditCommand {
    state: State,
    /// The polyline being edited.
    id: Option<EntityId>,
    /// The polyline as it stands, for the preview, which has no drawing.
    polyline: Option<PolylineEntity>,
    /// The polyline as it was before each change this command made, for
    /// `Undo` within the command.
    history: Vec<PolylineEntity>,
}

impl PeditCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Pick,
            id: None,
            polyline: None,
            history: Vec::new(),
        }
    }

    fn current(&mut self, ctx: &CommandContext<'_>) -> Option<PolylineEntity> {
        let polyline = match ctx.drawing.entity(self.id?) {
            Some(Entity::Polyline(polyline)) => Some(polyline.clone()),
            _ => None,
        };
        self.polyline = polyline.clone();
        polyline
    }

    /// Replaces the polyline with `changed`, remembering what it was.
    fn change(&mut self, ctx: &mut CommandContext<'_>, changed: PolylineEntity) {
        let (Some(id), Some(was)) = (self.id, self.current(ctx)) else {
            return;
        };
        let mut tx = ctx.transaction();
        if tx.replace(id, changed.clone()).is_ok() {
            tx.commit();
            self.history.push(was);
            self.polyline = Some(changed);
        }
    }

    /// A line or an arc as a polyline of one segment.
    fn as_polyline(entity: &Entity) -> Option<PolylineEntity> {
        match entity {
            Entity::Line(line) => Some(PolylineEntity::new(vec![line.start, line.end], false)),
            Entity::Arc(arc) => {
                let geometry = arc.arc();
                Some(PolylineEntity::with_bulges(
                    vec![geometry.start_point(), geometry.end_point()],
                    vec![(geometry.sweep / 4.0).tan(), 0.0],
                    false,
                ))
            }
            _ => None,
        }
    }

    /// The polyline joined with the selection: the chain that holds it.
    fn join_selection(&mut self, ctx: &mut CommandContext<'_>) {
        let Some(id) = self.id else {
            return;
        };
        let others: Vec<EntityId> = ctx.selection.iter().filter(|other| *other != id).collect();
        ctx.selection.clear();
        if others.is_empty() {
            ctx.message("No objects selected.");
            return;
        }
        let ids: Vec<EntityId> = std::iter::once(id).chain(others).collect();
        let entities: Vec<&Entity> = ids
            .iter()
            .filter_map(|id| ctx.drawing.entity(*id))
            .collect();
        if entities.len() != ids.len() {
            return;
        }
        let joined = match join(&entities) {
            Ok(results) => results
                .into_iter()
                .find(|result| result.members.contains(&0)),
            Err(_) => None,
        };
        let Some(joined) = joined else {
            ctx.message("Nothing joins to the polyline: no selected object meets its ends.");
            return;
        };
        let Entity::Polyline(polyline) = joined.entity else {
            return;
        };
        let was = self.current(ctx);
        let added = joined.members.len() - 1;
        let mut tx = ctx.transaction();
        let mut ok = tx.replace(id, polyline).is_ok();
        for member in &joined.members {
            if *member != 0 {
                ok &= tx.remove(ids[*member]).is_ok();
            }
        }
        if ok {
            tx.commit();
            if let Some(was) = was {
                self.history.push(was);
            }
            self.current(ctx);
            ctx.message(format!(
                "{} added to the polyline.",
                counted(added, "object")
            ));
        } else {
            drop(tx);
            ctx.message("The polyline could not be joined.");
        }
    }

    /// The polyline with `to` as the vertex `index`, or inserted after it.
    fn with_vertex(
        polyline: &PolylineEntity,
        index: usize,
        to: Point2,
        insert: bool,
    ) -> PolylineEntity {
        let mut vertices = polyline.vertices.clone();
        let mut bulges: Vec<f64> = polyline.bulges().collect();
        if insert {
            vertices.insert(index + 1, to);
            // The segment from the marked vertex keeps its bulge to the
            // new vertex; the new vertex starts a straight segment.
            bulges.insert(index + 1, 0.0);
        } else {
            vertices[index] = to;
        }
        PolylineEntity::with_bulges(vertices, bulges, polyline.closed).with_width(polyline.width())
    }

    fn vertex_count(&mut self, ctx: &CommandContext<'_>) -> usize {
        self.current(ctx)
            .map_or(0, |polyline| polyline.vertices.len())
    }
}

impl Command for PeditCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Pick => Prompt::new("Select polyline"),
            State::Convert(_) => {
                Prompt::new("Object selected is not a polyline. Turn it into one?")
                    .with_default("Yes")
            }
            State::Options => Prompt::new("Enter an option"),
            State::JoinSelect => Prompt::new("Select objects to join"),
            State::Width => Prompt::new("Specify new width for all segments"),
            State::Vertex { .. } => {
                Prompt::new("Enter a vertex editing option").with_default("Next")
            }
            State::VertexInsert { .. } => Prompt::new("Specify location of new vertex"),
            State::VertexMove { .. } => Prompt::new("Specify new location for marked vertex"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Pick => ExpectedInput::new(InputKind::Object),
            State::Convert(_) => ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO]),
            State::Options => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::JoinSelect => selection_expected(),
            State::Width => ExpectedInput::distance(),
            State::Vertex { .. } => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&VERTEX_OPTIONS)
            }
            State::VertexInsert { .. } | State::VertexMove { .. } => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Pick, CommandInput::Pick { entity, .. }) => match ctx.drawing.entity(entity) {
                Some(Entity::Polyline(_)) => {
                    self.id = Some(entity);
                    ctx.selection.clear();
                    self.current(ctx);
                    self.state = State::Options;
                }
                Some(Entity::Line(_) | Entity::Arc(_)) => self.state = State::Convert(entity),
                Some(other) => {
                    ctx.message(format!(
                        "A {} cannot be edited as a polyline.",
                        other.kind_name()
                    ));
                }
                None => {}
            },
            (State::Convert(id), CommandInput::Keyword(NO)) => {
                let _ = id;
                self.state = State::Pick;
            }
            (State::Convert(id), CommandInput::Keyword(_) | CommandInput::Accept) => {
                let id = *id;
                if let Some(polyline) = ctx.drawing.entity(id).and_then(Self::as_polyline) {
                    let mut tx = ctx.transaction();
                    if tx.replace(id, polyline).is_ok() {
                        tx.commit();
                        self.id = Some(id);
                        ctx.selection.clear();
                        self.current(ctx);
                        self.state = State::Options;
                    }
                }
            }
            (State::Options, CommandInput::Keyword(keyword)) => {
                let Some(polyline) = self.current(ctx) else {
                    return Step::Finished;
                };
                match keyword {
                    CLOSE => {
                        if polyline.closed {
                            ctx.message("The polyline is already closed.");
                        } else if polyline.vertices.len() < 3 {
                            ctx.message("A polyline needs three vertices to close.");
                        } else {
                            self.change(ctx, polyline.with_closed(true));
                        }
                    }
                    OPEN => {
                        if !polyline.closed {
                            ctx.message("The polyline is already open.");
                        } else {
                            self.change(ctx, polyline.with_closed(false));
                        }
                    }
                    JOIN => {
                        if polyline.closed {
                            ctx.message("A closed polyline has no ends to join to.");
                        } else {
                            self.state = State::JoinSelect;
                        }
                    }
                    WIDTH => self.state = State::Width,
                    EDIT_VERTEX => self.state = State::Vertex { index: 0 },
                    DECURVE => {
                        let straight =
                            PolylineEntity::new(polyline.vertices.clone(), polyline.closed)
                                .with_width(polyline.width());
                        self.change(ctx, straight);
                    }
                    _ => match (self.id, self.history.pop()) {
                        (Some(id), Some(was)) => {
                            let mut tx = ctx.transaction();
                            tx.replace(id, was.clone()).expect("the polyline is live");
                            tx.commit();
                            self.polyline = Some(was);
                        }
                        _ => ctx.message("Nothing to undo."),
                    },
                }
            }
            (State::Options, CommandInput::Accept) => return Step::Finished,
            (State::JoinSelect, CommandInput::Accept) => {
                self.join_selection(ctx);
                self.state = State::Options;
            }
            (State::JoinSelect, _) => {}
            (State::Width, CommandInput::Distance(width)) => {
                if !(width.is_finite() && width >= 0.0) {
                    ctx.message("The width must be zero or a positive number.");
                } else if let Some(polyline) = self.current(ctx) {
                    self.change(ctx, polyline.with_width(width));
                    self.state = State::Options;
                }
            }
            (State::Width, CommandInput::Accept) => self.state = State::Options,
            (State::Vertex { index }, CommandInput::Keyword(keyword)) => {
                let (index, count) = (*index, self.vertex_count(ctx));
                let Some(polyline) = self.current(ctx) else {
                    return Step::Finished;
                };
                match keyword {
                    PREVIOUS => {
                        self.state = State::Vertex {
                            index: if index == 0 { count - 1 } else { index - 1 },
                        };
                    }
                    INSERT => self.state = State::VertexInsert { index },
                    MOVE => self.state = State::VertexMove { index },
                    STRAIGHTEN => {
                        let mut bulges: Vec<f64> = polyline.bulges().collect();
                        if bulges.get(index).is_some_and(|bulge| *bulge != 0.0) {
                            bulges[index] = 0.0;
                            self.change(
                                ctx,
                                PolylineEntity::with_bulges(
                                    polyline.vertices.clone(),
                                    bulges,
                                    polyline.closed,
                                )
                                .with_width(polyline.width()),
                            );
                        } else {
                            ctx.message("The segment from the marked vertex is straight.");
                        }
                    }
                    EXIT => self.state = State::Options,
                    _ => {
                        self.state = State::Vertex {
                            index: (index + 1) % count,
                        }
                    }
                }
            }
            (State::Vertex { index }, CommandInput::Accept) => {
                self.state = State::Vertex {
                    index: (*index + 1) % self.vertex_count(ctx).max(1),
                };
            }
            (State::VertexInsert { index }, CommandInput::Point(to)) => {
                let index = *index;
                if let Some(polyline) = self.current(ctx) {
                    self.change(ctx, Self::with_vertex(&polyline, index, to, true));
                }
                self.state = State::Vertex { index: index + 1 };
            }
            (State::VertexMove { index }, CommandInput::Point(to)) => {
                let index = *index;
                if let Some(polyline) = self.current(ctx) {
                    self.change(ctx, Self::with_vertex(&polyline, index, to, false));
                }
                self.state = State::Vertex { index };
            }
            (State::VertexInsert { index } | State::VertexMove { index }, CommandInput::Accept) => {
                self.state = State::Vertex { index: *index };
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// The polyline highlighted while it is edited; the marked vertex as
    /// a hot grip; the polyline as a point would leave it.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let Some(polyline) = &self.polyline else {
            return;
        };
        if let Some(id) = self.id {
            out.push(OverlayItem::Highlight(id));
        }
        match &self.state {
            State::Vertex { index } => {
                if let Some(at) = polyline.vertices.get(*index) {
                    out.push(OverlayItem::HotGrip { at: *at });
                }
            }
            State::VertexInsert { index } => {
                out.push(OverlayItem::PreviewEntity(
                    Self::with_vertex(polyline, *index, cursor, true).into(),
                ));
            }
            State::VertexMove { index } => {
                out.push(OverlayItem::PreviewEntity(
                    Self::with_vertex(polyline, *index, cursor, false).into(),
                ));
            }
            _ => {}
        }
    }

    fn transient_sources(&self) -> &[EntityId] {
        match (&self.state, &self.id) {
            (State::VertexInsert { .. } | State::VertexMove { .. }, Some(id)) => {
                std::slice::from_ref(id)
            }
            _ => &[],
        }
    }
}
