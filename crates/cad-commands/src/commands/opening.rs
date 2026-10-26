//! WINDOW and DOOR (#325): an opening through a wall already drawn — a
//! floor plan's (`FLOORPLAN`), or a drafter's own two parallel lines. One
//! face of the wall is picked; the command finds the face across from it,
//! asks where the opening's centre goes and how wide it is — a door, which
//! room it opens into — and cuts both faces, closes the opening with a
//! jamb at each end, and draws a window's glass on `WINDOW` or a door's
//! leaf and swing on `DOOR`, as `crate::floorplan` draws them, in one
//! transaction. The geometry is `cad_geometry::Opening`; the faces are cut
//! as `BREAK` cuts, so a polyline face — a plan's outer face — is cut as
//! well as a line. The jambs take the face's own properties; what is not a
//! wall, or an opening that would reach past a face's end, is refused
//! with the reason.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, EntityId, Point2};
use cad_document::{ArcEntity, Entity, Layer, LineEntity, PolylineEntity, Properties};
use cad_geometry::{Opening, OpeningError, Primitive, Segment};
use cad_input::ExpectedInput;

use super::ltscale::display;
use crate::floorplan::{DOORS, WINDOWS};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

/// What the opening holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpeningKind {
    Window,
    Door,
}

/// A window's width, a door's, and a door leaf's thickness, in metres.
const WINDOW_WIDTH: f64 = 1.2;
const DOOR_WIDTH: f64 = 0.9;
const LEAF_THICKNESS: f64 = 0.05;
/// The thickest wall a face across from the picked one is looked for
/// within, in metres.
const THICKEST_WALL: f64 = 1.0;

/// The wall: the face picked and the face across from it, each the
/// straight stretch the opening cuts.
#[derive(Clone, Copy, Debug)]
struct Wall {
    face: EntityId,
    first: Segment,
    mate: EntityId,
    second: Segment,
}

#[derive(Debug)]
enum State {
    Face,
    Centre {
        wall: Wall,
    },
    Width {
        wall: Wall,
        centre: Point2,
    },
    /// A door: which side it opens into.
    Into {
        wall: Wall,
        opening: Opening,
    },
}

#[derive(Debug)]
pub(crate) struct OpeningCommand {
    kind: OpeningKind,
    state: State,
    /// Drawing units to the metre.
    scale: f64,
}

impl OpeningCommand {
    pub(crate) fn new(kind: OpeningKind) -> Self {
        Self {
            kind,
            state: State::Face,
            scale: 1.0,
        }
    }

    fn default_width(&self) -> f64 {
        match self.kind {
            OpeningKind::Window => WINDOW_WIDTH,
            OpeningKind::Door => DOOR_WIDTH,
        }
    }

    fn what(&self) -> &'static str {
        match self.kind {
            OpeningKind::Window => "window",
            OpeningKind::Door => "door",
        }
    }

    fn tolerance(&self, near: Point2) -> LinearTolerance {
        LinearTolerance::DEFAULT.around(&[near])
    }

    /// The opening of `width` centred at `centre` in `wall`, or why not.
    fn opening(&self, wall: &Wall, centre: Point2, width: f64) -> Result<Opening, OpeningError> {
        Opening::new(
            wall.first,
            wall.second,
            centre,
            width,
            self.tolerance(centre),
        )
    }

    fn refuse(&self, ctx: &mut CommandContext<'_>, error: OpeningError) {
        let what = self.what();
        ctx.message(match error {
            OpeningError::OffTheWall => format!("Put the {what}'s centre on the wall."),
            OpeningError::TooWide { fits } => format!(
                "Only {} fits there: move the {what} away from the wall's end, or make it narrower.",
                display(fits)
            ),
            OpeningError::BadWidth => format!("A {what}'s width must be greater than zero."),
            OpeningError::NotParallel | OpeningError::NoThickness | OpeningError::NoOverlap => {
                "Those faces are not a wall.".to_owned()
            }
        });
    }

    /// Cuts both faces, closes the opening, and draws what it holds.
    fn commit(
        &self,
        wall: &Wall,
        opening: &Opening,
        into: Option<Point2>,
        ctx: &mut CommandContext<'_>,
    ) -> Step {
        let cuts = opening.cuts();
        let mut broken = Vec::new();
        for (id, (p, q)) in [(wall.face, cuts[0]), (wall.mate, cuts[1])] {
            let Some(entity) = ctx.drawing.entity(id) else {
                ctx.message("The wall is gone.");
                return Step::Finished;
            };
            match entity.broken(p, q) {
                Ok(pieces) => broken.push((id, pieces)),
                Err(_) => {
                    ctx.message("That face cannot be cut there.");
                    return Step::Continue;
                }
            }
        }
        let layer_name = match self.kind {
            OpeningKind::Window => WINDOWS,
            OpeningKind::Door => DOORS,
        };
        let found = ctx.drawing.layers().find(layer_name);
        let mut transaction = ctx.transaction();
        for (id, pieces) in broken {
            let mut pieces = pieces.into_iter();
            if let Some(first) = pieces.next() {
                transaction
                    .replace(id, first)
                    .expect("the face was just read");
            }
            for piece in pieces {
                transaction
                    .insert_like(id, piece)
                    .expect("the face was just read");
            }
        }
        for (a, b) in opening.jambs() {
            transaction
                .insert_like(wall.face, LineEntity::new(a, b))
                .expect("the face was just read");
        }
        let layer = match found {
            Some(id) => id,
            None => transaction
                .add_layer(Layer::named(layer_name))
                .expect("a layer the drawing has not got"),
        };
        let properties = Properties {
            layer,
            ..Properties::default()
        };
        let drawn: Vec<Entity> = match (self.kind, into) {
            (OpeningKind::Window, _) => opening
                .glass()
                .into_iter()
                .map(|(a, b)| LineEntity::new(a, b).into())
                .collect(),
            (OpeningKind::Door, Some(into)) => {
                let leaf = opening.leaf(into, LEAF_THICKNESS * self.scale);
                vec![
                    PolylineEntity::new(leaf.outline.to_vec(), true).into(),
                    ArcEntity::new(leaf.hinge, leaf.radius, leaf.swing.0, leaf.swing.1).into(),
                ]
            }
            (OpeningKind::Door, None) => Vec::new(),
        };
        for entity in drawn {
            transaction
                .insert_with(entity, properties)
                .expect("the layer was found or made");
        }
        transaction.commit();
        Step::Finished
    }

    /// What is previewed at `cursor`: the opening's jambs and glass, or a
    /// door's leaf once its side is asked.
    fn preview_at(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let default = self.default_width() * self.scale;
        let (wall, opening) = match &self.state {
            State::Face => return,
            State::Centre { wall } => (wall, self.opening(wall, cursor, default)),
            State::Width { wall, centre } => (wall, self.opening(wall, *centre, default)),
            State::Into { wall, opening } => (wall, Ok(*opening)),
        };
        out.push(OverlayItem::Highlight(wall.face));
        out.push(OverlayItem::Highlight(wall.mate));
        let Ok(opening) = opening else {
            return;
        };
        let line =
            |(a, b): (Point2, Point2)| OverlayItem::PreviewEntity(LineEntity::new(a, b).into());
        out.extend(opening.jambs().map(line));
        match (self.kind, &self.state) {
            (OpeningKind::Window, _) => out.extend(opening.glass().map(line)),
            (OpeningKind::Door, State::Into { .. }) => {
                let leaf = opening.leaf(cursor, LEAF_THICKNESS * self.scale);
                out.push(OverlayItem::PreviewEntity(
                    PolylineEntity::new(leaf.outline.to_vec(), true).into(),
                ));
                out.push(OverlayItem::PreviewEntity(
                    ArcEntity::new(leaf.hinge, leaf.radius, leaf.swing.0, leaf.swing.1).into(),
                ));
            }
            (OpeningKind::Door, _) => {}
        }
    }
}

/// The straight stretch of `entity` nearest `at`, if it has one.
fn straight_near(entity: &Entity, at: Point2, ctx: &CommandContext<'_>) -> Option<Segment> {
    entity
        .primitives(ctx.drawing.tables())
        .into_iter()
        .filter_map(|primitive| match primitive {
            Primitive::Segment(segment) => Some(segment),
            _ => None,
        })
        .min_by(|a, b| {
            a.closest_point(at)
                .distance_to(at)
                .total_cmp(&b.closest_point(at).distance_to(at))
        })
}

/// How far `candidate` lies across from `first`, when it is a face of the
/// same wall: parallel, apart, and sharing a stretch.
fn across(first: Segment, candidate: Segment, tolerance: LinearTolerance) -> Option<f64> {
    let a = first.end - first.start;
    let b = candidate.end - candidate.start;
    let (la, lb) = (a.length(), b.length());
    if la <= tolerance.value() || lb <= tolerance.value() {
        return None;
    }
    let along = a * (1.0 / la);
    let sine = (along.x * b.y - along.y * b.x) / lb;
    if sine.abs() * la.max(lb) > tolerance.value() * 1e3 {
        return None;
    }
    let normal = cad_core::Vec2::new(-along.y, along.x);
    let offset = (candidate.start - first.start).dot(normal).abs();
    if offset <= tolerance.value() {
        return None;
    }
    let t = |p: Point2| (p - first.start).dot(along);
    let (s, e) = (t(candidate.start), t(candidate.end));
    (s.min(e) < la - tolerance.value() && s.max(e) > tolerance.value()).then_some(offset)
}

impl Command for OpeningCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.scale = ctx.drawing.unit().map_or(1.0, |unit| 1.0 / unit.metres());
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        let what = self.what();
        match &self.state {
            State::Face => Prompt::new("Select a face of the wall"),
            State::Centre { .. } => Prompt::new(format!("Specify the {what}'s centre on the wall")),
            State::Width { .. } => Prompt::new(format!("Specify {what} width"))
                .with_default(display(self.default_width() * self.scale)),
            State::Into { .. } => Prompt::new("Specify a point on the side the door opens into"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Face => ExpectedInput::object(),
            State::Centre { .. } | State::Into { .. } => ExpectedInput::point(),
            State::Width { .. } => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Face);
        self.state = match (state, input) {
            (State::Face, CommandInput::Pick { entity, at }) => {
                let Some(first) = ctx
                    .drawing
                    .entity(entity)
                    .and_then(|e| straight_near(e, at, ctx))
                else {
                    ctx.message("Select a straight face of a wall: a line, or a straight stretch of a polyline.");
                    return Step::Continue;
                };
                let tolerance = self.tolerance(at);
                let reach = THICKEST_WALL * self.scale;
                let bounds = Aabb::from_points(at, at).expanded(reach);
                let mate = ctx
                    .drawing
                    .query_shown_in(ctx.space, &bounds)
                    .into_iter()
                    .filter(|id| *id != entity)
                    .filter_map(|id| {
                        let other = ctx.drawing.entity(id)?;
                        other
                            .primitives(ctx.drawing.tables())
                            .into_iter()
                            .filter_map(|primitive| match primitive {
                                Primitive::Segment(segment) => Some(segment),
                                _ => None,
                            })
                            .filter_map(|segment| {
                                across(first, segment, tolerance)
                                    .filter(|offset| *offset <= reach)
                                    .map(|offset| (offset, id, segment))
                            })
                            .min_by(|a, b| a.0.total_cmp(&b.0))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                match mate {
                    Some((_, mate, second)) => State::Centre {
                        wall: Wall {
                            face: entity,
                            first,
                            mate,
                            second,
                        },
                    },
                    None => {
                        ctx.message(
                            "No face lies across from that one: a wall is two parallel faces.",
                        );
                        State::Face
                    }
                }
            }
            (State::Face, CommandInput::Accept) => return Step::Finished,
            (State::Centre { wall }, CommandInput::Point(centre)) => {
                // The centre is checked now, so the width is asked only
                // where an opening can go.
                match self.opening(&wall, centre, self.tolerance(centre).value() * 10.0) {
                    Ok(_) => State::Width { wall, centre },
                    Err(error) => {
                        self.refuse(ctx, error);
                        State::Centre { wall }
                    }
                }
            }
            (State::Width { wall, centre }, CommandInput::Accept) => {
                let width = self.default_width() * self.scale;
                return self.sized(wall, centre, width, ctx);
            }
            (State::Width { wall, centre }, CommandInput::Distance(width)) => {
                return self.sized(wall, centre, width, ctx);
            }
            (State::Into { wall, opening }, CommandInput::Point(into)) => {
                return self.commit(&wall, &opening, Some(into), ctx);
            }
            (state, _) => state,
        };
        Step::Continue
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Width { centre, .. } => Some(*centre),
            State::Into { opening, .. } => Some(opening.from),
            _ => None,
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        self.preview_at(cursor, out);
    }
}

impl OpeningCommand {
    /// The width given: a window is drawn; a door asks its side first.
    fn sized(
        &mut self,
        wall: Wall,
        centre: Point2,
        width: f64,
        ctx: &mut CommandContext<'_>,
    ) -> Step {
        match self.opening(&wall, centre, width) {
            Ok(opening) => match self.kind {
                OpeningKind::Window => self.commit(&wall, &opening, None, ctx),
                OpeningKind::Door => {
                    self.state = State::Into { wall, opening };
                    Step::Continue
                }
            },
            Err(error) => {
                self.refuse(ctx, error);
                self.state = State::Width { wall, centre };
                Step::Continue
            }
        }
    }
}
