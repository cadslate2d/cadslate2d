//! POLYGON (#201): a regular polygon as a closed polyline — by a centre
//! and a radius, inscribed in that circle or circumscribed about it, or
//! by one edge — on the current properties.

use cad_core::Point2;
use cad_document::PolylineEntity;
use cad_geometry::{polygon_from_edge, regular_polygon};
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const EDGE: Keyword = Keyword::new("Edge");
const INSCRIBED: Keyword = Keyword::new("Inscribed in circle");
const CIRCUMSCRIBED: Keyword = Keyword::new("Circumscribed about circle");
const SIDES_LIMIT: f64 = 1024.0;

#[derive(Debug)]
enum State {
    Sides,
    Center {
        sides: usize,
    },
    Fit {
        sides: usize,
        center: Point2,
    },
    Radius {
        sides: usize,
        center: Point2,
        inscribed: bool,
    },
    EdgeFirst {
        sides: usize,
    },
    EdgeSecond {
        sides: usize,
        first: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct PolygonCommand {
    state: State,
    /// The sides last asked for: the next default.
    sides: usize,
}

impl PolygonCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Sides,
            sides: 4,
        }
    }

    /// The polygon the state and `point` — the radius point or the
    /// edge's second end — would make.
    fn corners(&self, point: Point2) -> Option<Vec<Point2>> {
        match &self.state {
            State::Radius {
                sides,
                center,
                inscribed,
            } => {
                let reach = point - *center;
                let radius = reach.length();
                let angle = reach.y.atan2(reach.x);
                // Inscribed, the first corner is at the point; circumscribed,
                // the first side's middle is.
                let start = if *inscribed {
                    angle
                } else {
                    angle - std::f64::consts::PI / *sides as f64
                };
                regular_polygon(*center, *sides, radius, *inscribed, start)
            }
            State::EdgeSecond { sides, first } => polygon_from_edge(*first, point, *sides),
            _ => None,
        }
    }

    fn finish(&self, ctx: &mut CommandContext<'_>, corners: Vec<Point2>) -> Step {
        let mut tx = ctx.transaction();
        tx.insert(PolylineEntity::new(corners, true))
            .expect("the current layer and linetype exist");
        tx.commit();
        Step::Finished
    }
}

impl Command for PolygonCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Sides => {
                Prompt::new("Enter number of sides").with_default(self.sides.to_string())
            }
            State::Center { .. } => Prompt::new("Specify center of polygon"),
            State::Fit { .. } => Prompt::new("Enter an option").with_default("Inscribed in circle"),
            State::Radius { .. } => Prompt::new("Specify radius of circle"),
            State::EdgeFirst { .. } => Prompt::new("Specify first endpoint of edge"),
            State::EdgeSecond { .. } => Prompt::new("Specify second endpoint of edge"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Sides => ExpectedInput::factor(),
            State::Center { .. } => ExpectedInput::point().with_keywords(&[EDGE]),
            State::Fit { .. } => ExpectedInput::new(cad_input::InputKind::Nothing)
                .with_keywords(&[INSCRIBED, CIRCUMSCRIBED]),
            State::Radius { .. } => ExpectedInput::distance(),
            State::EdgeFirst { .. } | State::EdgeSecond { .. } => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Sides, CommandInput::Distance(sides)) => {
                let whole = sides.round();
                if !(sides.is_finite()
                    && (sides - whole).abs() < 1e-9
                    && (3.0..=SIDES_LIMIT).contains(&whole))
                {
                    ctx.message("Enter a whole number of sides from 3 to 1024.");
                    return Step::Continue;
                }
                self.sides = whole as usize;
                self.state = State::Center { sides: self.sides };
            }
            (State::Sides, CommandInput::Accept) => {
                self.state = State::Center { sides: self.sides };
            }
            (State::Center { sides }, CommandInput::Keyword(_)) => {
                self.state = State::EdgeFirst { sides: *sides };
            }
            (State::Center { sides }, CommandInput::Point(center)) => {
                self.state = State::Fit {
                    sides: *sides,
                    center,
                };
            }
            (State::Fit { sides, center }, CommandInput::Keyword(keyword)) => {
                self.state = State::Radius {
                    sides: *sides,
                    center: *center,
                    inscribed: keyword == INSCRIBED,
                };
            }
            (State::Fit { sides, center }, CommandInput::Accept) => {
                self.state = State::Radius {
                    sides: *sides,
                    center: *center,
                    inscribed: true,
                };
            }
            (State::Radius { center, .. }, CommandInput::Distance(radius)) => {
                // A typed radius: the first corner (or side) straight to the
                // right of the centre.
                let point = *center + cad_core::Vec2::new(radius, 0.0);
                return match self.corners(point) {
                    Some(corners) => self.finish(ctx, corners),
                    None => {
                        ctx.message("The radius must be a positive number.");
                        Step::Continue
                    }
                };
            }
            (State::Radius { .. }, CommandInput::Point(point)) => {
                return match self.corners(point) {
                    Some(corners) => self.finish(ctx, corners),
                    None => {
                        ctx.message("The radius must be a positive number.");
                        Step::Continue
                    }
                };
            }
            (State::EdgeFirst { sides }, CommandInput::Point(first)) => {
                self.state = State::EdgeSecond {
                    sides: *sides,
                    first,
                };
            }
            (State::EdgeSecond { .. }, CommandInput::Point(second)) => {
                return match self.corners(second) {
                    Some(corners) => self.finish(ctx, corners),
                    None => {
                        ctx.message("The edge needs two different points.");
                        Step::Continue
                    }
                };
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// The polygon as the pointer would make it.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(corners) = self.corners(cursor) {
            out.push(OverlayItem::PreviewEntity(
                PolylineEntity::new(corners, true).into(),
            ));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Radius { center, .. } => Some(*center),
            State::EdgeSecond { first, .. } => Some(*first),
            _ => None,
        }
    }
}
