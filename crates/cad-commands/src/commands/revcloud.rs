//! REVCLOUD (#215): a revision cloud round a rectangle, a polygon, or a
//! closed object.
//!
//! The cloud is a closed polyline of arcs (`Path::revision_cloud`), so
//! every polyline command, `PEDIT`, and DXF take it as it is. `Arc
//! length` sets the shortest and the longest arc, kept for the session;
//! `REVerse` turns the arcs to bow inwards; `Object` turns a closed
//! polyline, a circle, a whole ellipse, or a closed spline into a cloud
//! along it, in its place. One cloud, one transaction.

use cad_core::Point2;
use cad_document::{Entity, PolylineEntity};
use cad_geometry::{MAX_CLOUD_ARCS, Path};
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const ARC_LENGTH: Keyword = Keyword::new("Arc length");
const OBJECT: Keyword = Keyword::new("Object");
const RECTANGULAR: Keyword = Keyword::new("Rectangular");
const POLYGONAL: Keyword = Keyword::new("Polygonal");
const REVERSE: Keyword = Keyword::new("REVerse");
const OPTIONS: [Keyword; 5] = [ARC_LENGTH, OBJECT, RECTANGULAR, POLYGONAL, REVERSE];
const UNDO: Keyword = Keyword::new("Undo");

/// How the outline is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Rectangular,
    Polygonal,
}

#[derive(Debug)]
enum State {
    /// The first point, or an option.
    Start,
    /// The rectangle's opposite corner.
    Corner(Point2),
    /// The polygon's next point; Enter closes it.
    Next(Vec<Point2>),
    /// The shortest arc, then the longest.
    Shortest,
    Longest(f64),
    /// The object to cloud.
    Object,
}

#[derive(Debug)]
pub(crate) struct RevCloudCommand {
    state: State,
    mode: Mode,
    /// The shortest and longest arc, as the command found them.
    arcs: [f64; 2],
    inwards: bool,
}

/// A closed outline through `points`.
fn polygon(points: Vec<Point2>) -> Path {
    Path {
        vertices: points,
        bulges: Vec::new(),
        closed: true,
    }
}

/// The rectangle with corners `a` and `b`.
fn rectangle(a: Point2, b: Point2) -> Path {
    polygon(vec![a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)])
}

/// The outline of an object a cloud can follow, or why it cannot.
fn outline(entity: &Entity) -> Result<Path, String> {
    match entity {
        Entity::Polyline(polyline) if polyline.closed => Ok(polyline.path()),
        Entity::Polyline(_) => Err("The polyline is open: a cloud needs a closed outline.".into()),
        Entity::Circle(circle) => {
            let (center, radius) = (circle.center, circle.radius);
            Ok(Path {
                vertices: vec![
                    Point2::new(center.x + radius, center.y),
                    Point2::new(center.x - radius, center.y),
                ],
                bulges: vec![1.0, 1.0],
                closed: true,
            })
        }
        Entity::Ellipse(ellipse) if ellipse.is_full() => Ok(polygon(
            ellipse
                .ellipse()
                .segments()
                .iter()
                .map(|segment| segment.start)
                .collect(),
        )),
        Entity::Spline(spline) if spline.is_closed() => Ok(polygon(
            spline
                .spline()
                .segments()
                .iter()
                .map(|segment| segment.start)
                .collect(),
        )),
        other => Err(format!(
            "A {} is not a closed outline: pick a closed polyline, a circle, an ellipse, or a closed spline.",
            other.kind_name()
        )),
    }
}

impl RevCloudCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Start,
            mode: Mode::Rectangular,
            arcs: [7.5, 15.0],
            inwards: false,
        }
    }

    fn cloud(&self, outline: &Path) -> Option<Path> {
        outline.revision_cloud(self.arcs[0], self.arcs[1], self.inwards)
    }

    /// Adds the cloud along `outline`, or says why there is none.
    fn commit(&self, outline: &Path, ctx: &mut CommandContext<'_>) -> Step {
        let Some(cloud) = self.cloud(outline) else {
            ctx.message(self.refusal(outline));
            return Step::Continue;
        };
        let arcs = cloud.vertices.len();
        let mut transaction = ctx.transaction();
        transaction
            .insert(PolylineEntity::from(cloud))
            .expect("a closed polyline of finite points");
        transaction.commit();
        ctx.message(format!("Revision cloud made: {arcs} arcs."));
        Step::Finished
    }

    /// Why an outline has no cloud: more arcs than a cloud may have, or
    /// nothing enclosed.
    fn refusal(&self, outline: &Path) -> &'static str {
        if outline.length() / self.arcs[1] > MAX_CLOUD_ARCS as f64 {
            "The arcs are too short for so long an outline: make them longer with Arc length."
        } else {
            "A cloud needs an outline that encloses something."
        }
    }
}

impl Command for RevCloudCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.arcs = ctx.settings.cloud_arcs;
        ctx.message(format!(
            "Arc length: shortest {}, longest {}.",
            self.arcs[0], self.arcs[1]
        ));
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Start => match self.mode {
                Mode::Rectangular => Prompt::new("Specify first corner point"),
                Mode::Polygonal => Prompt::new("Specify start point"),
            }
            .with_default("Object"),
            State::Corner(_) => Prompt::new("Specify opposite corner"),
            State::Next(points) if points.len() < 3 => Prompt::new("Specify next point"),
            State::Next(_) => Prompt::new("Specify next point").with_default("close"),
            State::Shortest => {
                Prompt::new("Specify shortest arc").with_default(self.arcs[0].to_string())
            }
            State::Longest(_) => {
                Prompt::new("Specify longest arc").with_default(self.arcs[1].to_string())
            }
            State::Object => Prompt::new("Select a closed object to turn into a cloud"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Start => ExpectedInput::point().with_keywords(&OPTIONS),
            State::Corner(_) => ExpectedInput::point(),
            State::Next(points) if points.len() > 1 => {
                ExpectedInput::point().with_keywords(&[UNDO])
            }
            State::Next(_) => ExpectedInput::point(),
            State::Shortest | State::Longest(_) => ExpectedInput::distance(),
            State::Object => ExpectedInput::object(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Start);
        self.state = match (state, input) {
            (State::Start, CommandInput::Point(point)) => match self.mode {
                Mode::Rectangular => State::Corner(point),
                Mode::Polygonal => State::Next(vec![point]),
            },
            (State::Start, CommandInput::Keyword(ARC_LENGTH)) => State::Shortest,
            (State::Start, CommandInput::Keyword(OBJECT) | CommandInput::Accept) => State::Object,
            (State::Start, CommandInput::Keyword(RECTANGULAR)) => {
                self.mode = Mode::Rectangular;
                State::Start
            }
            (State::Start, CommandInput::Keyword(POLYGONAL)) => {
                self.mode = Mode::Polygonal;
                State::Start
            }
            (State::Start, CommandInput::Keyword(_)) => {
                self.inwards = !self.inwards;
                ctx.message(if self.inwards {
                    "The arcs bow inwards."
                } else {
                    "The arcs bow outwards."
                });
                State::Start
            }
            (State::Corner(first), CommandInput::Point(second)) => {
                let step = self.commit(&rectangle(first, second), ctx);
                if step == Step::Finished {
                    return step;
                }
                State::Corner(first)
            }
            (State::Next(mut points), CommandInput::Point(point)) => {
                points.push(point);
                State::Next(points)
            }
            (State::Next(mut points), CommandInput::Keyword(_)) => {
                points.pop();
                State::Next(points)
            }
            (State::Next(points), CommandInput::Accept) if points.len() >= 3 => {
                let step = self.commit(&polygon(points.clone()), ctx);
                if step == Step::Finished {
                    return step;
                }
                State::Next(points)
            }
            (State::Next(points), CommandInput::Accept) => {
                ctx.message("A cloud needs three points at least.");
                State::Next(points)
            }
            (State::Shortest, CommandInput::Distance(shortest)) if shortest > 0.0 => {
                State::Longest(shortest)
            }
            (State::Shortest, CommandInput::Accept) => State::Longest(self.arcs[0]),
            (State::Longest(shortest), CommandInput::Distance(longest)) if longest >= shortest => {
                self.set_arcs([shortest, longest], ctx);
                State::Start
            }
            (State::Longest(shortest), CommandInput::Accept) if self.arcs[1] >= shortest => {
                self.set_arcs([shortest, self.arcs[1]], ctx);
                State::Start
            }
            (State::Shortest, CommandInput::Distance(_)) => {
                ctx.message("An arc must be longer than zero.");
                State::Shortest
            }
            (State::Longest(shortest), CommandInput::Distance(_) | CommandInput::Accept) => {
                ctx.message("The longest arc cannot be shorter than the shortest.");
                State::Longest(shortest)
            }
            (State::Object, CommandInput::Pick { entity, .. }) => {
                let Some(found) = ctx.drawing.entity(entity).cloned() else {
                    return Step::Continue;
                };
                match outline(&found) {
                    Err(why) => {
                        ctx.message(why);
                        State::Object
                    }
                    Ok(path) => match self.cloud(&path) {
                        None => {
                            ctx.message(self.refusal(&path));
                            State::Object
                        }
                        Some(cloud) => {
                            let arcs = cloud.vertices.len();
                            // A polyline keeps its width; anything else
                            // becomes a plain one, with its properties.
                            let polyline = match &found {
                                Entity::Polyline(polyline) => polyline.with_path(cloud),
                                _ => PolylineEntity::from(cloud),
                            };
                            let mut transaction = ctx.transaction();
                            transaction
                                .replace(entity, polyline)
                                .expect("a live entity, replaced once");
                            transaction.commit();
                            ctx.message(format!("Revision cloud made: {arcs} arcs."));
                            return Step::Finished;
                        }
                    },
                }
            }
            (State::Object, CommandInput::Accept) => return Step::Finished,
            (state, _) => state,
        };
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let outline = match &self.state {
            State::Corner(first) => rectangle(*first, cursor),
            State::Next(points) if !points.is_empty() => {
                let mut points = points.clone();
                points.push(cursor);
                if points.len() < 3 {
                    out.push(OverlayItem::PreviewSegment {
                        start: points[0],
                        end: cursor,
                    });
                    return;
                }
                polygon(points)
            }
            _ => return,
        };
        if let Some(cloud) = self.cloud(&outline) {
            out.push(OverlayItem::PreviewEntity(
                PolylineEntity::from(cloud).into(),
            ));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Next(points) => points.last().copied(),
            _ => None,
        }
    }

    fn tracking_base(&self) -> Option<Point2> {
        match &self.state {
            State::Corner(_) => None,
            _ => self.base_point(),
        }
    }
}

impl RevCloudCommand {
    fn set_arcs(&mut self, arcs: [f64; 2], ctx: &mut CommandContext<'_>) {
        self.arcs = arcs;
        ctx.settings.cloud_arcs = arcs;
        ctx.message(format!(
            "Arc length: shortest {}, longest {}.",
            arcs[0], arcs[1]
        ));
    }
}
