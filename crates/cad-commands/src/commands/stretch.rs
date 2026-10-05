//! STRETCH: moves the part of the drawing inside a crossing window and
//! leaves the rest where it is, so objects that reach into the window
//! change shape (DESIGN §17, §22, §31).
//!
//! The window is asked for by the command itself, corner by corner, because
//! it is not just a way of choosing objects: it decides which *ends* move.
//! What a window does to each kind of entity is `Entity::stretched`.

use cad_core::{Aabb, EntityId, Point2, Vec2};
use cad_document::{Entity, StretchHold};
use cad_geometry::Transform;
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step, TransientEdit};

/// What the window took hold of.
#[derive(Debug, Default)]
struct Held {
    window: Option<Aabb>,
    /// Entities with every defining point inside: they move as they are.
    whole: TransientEdit,
    /// Entities with only some inside, as they were when the window closed.
    part: Vec<(EntityId, Entity)>,
    /// Both kinds together: what must not be snapped to meanwhile.
    sources: Vec<EntityId>,
}

impl Held {
    /// Everything `window` crosses that has a defining point inside it.
    fn by(window: Aabb, ctx: &CommandContext<'_>) -> Self {
        let mut ids = ctx.drawing.query(&window);
        ids.sort_unstable();
        let mut held = Self {
            window: Some(window),
            ..Self::default()
        };
        let mut whole = Vec::new();
        for id in ids {
            let Some(entity) = ctx
                .drawing
                .entity(id)
                .filter(|entity| entity.crosses(&window))
            else {
                continue;
            };
            match entity.stretch_hold(&window) {
                StretchHold::Nothing => continue,
                StretchHold::Whole => whole.push(id),
                StretchHold::Part => held.part.push((id, entity.clone())),
            }
            held.sources.push(id);
        }
        held.whole = TransientEdit::of_ids(whole);
        held
    }

    /// Each partly held entity as `offset` would leave it; `None` for one
    /// that would stop being a valid entity.
    fn reshaped(&self, offset: Vec2) -> impl Iterator<Item = (EntityId, Option<Entity>)> {
        self.part.iter().filter_map(move |(id, entity)| {
            Some((*id, entity.stretched(self.window.as_ref()?, offset)))
        })
    }
}

#[derive(Debug)]
enum State {
    FirstCorner,
    OppositeCorner { first: Point2 },
    BasePoint,
    SecondPoint { base: Point2 },
}

#[derive(Debug)]
pub(crate) struct StretchCommand {
    state: State,
    held: Held,
}

impl StretchCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::FirstCorner,
            held: Held::default(),
        }
    }

    fn commit(&self, base: Point2, to: Point2, ctx: &mut CommandContext<'_>) {
        let offset = to - base;
        if offset.length() == 0.0 {
            return;
        }
        let mut refused = 0;
        let mut changes: Vec<(EntityId, Entity)> = self
            .held
            .whole
            .sources()
            .iter()
            .filter_map(|id| Some((*id, ctx.drawing.entity(*id)?.translated(offset))))
            .collect();
        for (id, reshaped) in self.held.reshaped(offset) {
            match reshaped {
                Some(entity) => changes.push((id, entity)),
                None => refused += 1,
            }
        }
        if !changes.is_empty() {
            let mut transaction = ctx.drawing.transaction();
            for (id, entity) in changes {
                transaction
                    .replace(id, entity)
                    .expect("held entities are live and appear once");
            }
            transaction.commit();
        }
        match refused {
            0 => {}
            1 => ctx.message("1 object could not be stretched that way and was left unchanged."),
            several => ctx.message(format!(
                "{several} objects could not be stretched that way and were left unchanged."
            )),
        }
    }
}

impl Command for StretchCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match self.state {
            State::FirstCorner => "Specify first corner of crossing window",
            State::OppositeCorner { .. } => "Specify opposite corner",
            State::BasePoint => "Specify base point",
            State::SecondPoint { .. } => "Specify second point",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::SecondPoint { .. } => ExpectedInput::point().with_direct_distance(),
            _ => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::FirstCorner, CommandInput::Point(first)) => {
                self.state = State::OppositeCorner { first };
            }
            (State::OppositeCorner { first }, CommandInput::Point(opposite)) => {
                let held = Held::by(Aabb::from_points(*first, opposite), ctx);
                if held.sources.is_empty() {
                    ctx.message("Nothing in the window to stretch.");
                    self.state = State::FirstCorner;
                } else {
                    ctx.message(format!("{} found", held.sources.len()));
                    // Shown the way a selection is, and left selected if the
                    // command is cancelled, like any other edit's objects.
                    ctx.selection.clear();
                    for id in &held.sources {
                        ctx.selection.add(*id);
                    }
                    self.held = held;
                    self.state = State::BasePoint;
                }
            }
            (State::BasePoint, CommandInput::Point(base)) => {
                self.state = State::SecondPoint { base };
            }
            (State::SecondPoint { base }, CommandInput::Point(to)) => {
                self.commit(*base, to, ctx);
                ctx.selection.clear();
                return Step::Finished;
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        match self.state {
            State::OppositeCorner { first } => out.push(OverlayItem::SelectionBox {
                from: first,
                to: cursor,
                crossing: true,
            }),
            State::SecondPoint { base } => {
                let offset = cursor - base;
                self.held.whole.preview(Transform::translation(offset), out);
                out.extend(
                    self.held
                        .reshaped(offset)
                        .filter_map(|(_, entity)| entity)
                        .map(OverlayItem::PreviewEntity),
                );
                out.push(OverlayItem::PreviewSegment {
                    start: base,
                    end: cursor,
                });
            }
            State::FirstCorner | State::BasePoint => {}
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::SecondPoint { base } => Some(base),
            _ => None,
        }
    }

    /// The corners of the window are places on the screen, not points of
    /// the drawing: they are taken where the pointer is.
    fn takes_object_snap(&self) -> bool {
        !matches!(
            self.state,
            State::FirstCorner | State::OppositeCorner { .. }
        )
    }

    fn transient_sources(&self) -> &[EntityId] {
        match self.state {
            State::SecondPoint { .. } => &self.held.sources,
            _ => &[],
        }
    }
}
