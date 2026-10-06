//! DIMANGULAR, DIMRADIUS, and DIMDIAMETER (DESIGN §34; ADR-0020): the
//! dimensions of corners and arcs.
//!
//! DIMANGULAR takes an arc (its centre and ends), a circle (its centre,
//! the pick, and a second point), two lines (where they meet, and
//! whichever of the four angles between them the pointer is in), or Enter
//! and a vertex and two points; then where the dimension arc goes.
//! DIMRADIUS and DIMDIAMETER take an arc or a circle, then where the text
//! goes, which also sets the leader's angle. Each previews the dimension
//! the click would make and offers `Text` and `Angle`; each dimension is
//! one transaction, on the current layer in the current dimension style.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{DimStyleId, DimensionEntity, Entity, Measure};
use cad_geometry::{Segment, intersect::line_line, perpendicular};
use cad_input::ExpectedInput;

use super::dimtext::{OPTIONS, TextOptions, commit};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// What an angular dimension measures, before the placement decides
/// which angle at the vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Corner {
    /// A vertex and two points: the angle between the rays through them,
    /// or its reflex, whichever side the placement is on.
    Points {
        vertex: Point2,
        a: Point2,
        b: Point2,
    },
    /// Two lines through the vertex: whichever of the four angles between
    /// them the placement falls in.
    Lines {
        vertex: Point2,
        a: Point2,
        b: Point2,
    },
}

impl Corner {
    fn measure(self, place: Point2) -> Measure {
        match self {
            Corner::Points { vertex, a, b } => Measure::Angular { vertex, a, b },
            Corner::Lines { vertex, a, b } => Measure::between_lines(vertex, a, b, place),
        }
    }
}

#[derive(Debug)]
enum AngularState {
    /// An arc, a circle, or a line — or Enter for a vertex.
    Object,
    /// A circle was picked at `a`: the second point.
    SecondEndpoint {
        vertex: Point2,
        a: Point2,
    },
    /// A line was picked at `at`: the second line.
    SecondLine {
        first: Segment,
        at: Point2,
    },
    Vertex,
    First {
        vertex: Point2,
    },
    Second {
        vertex: Point2,
        a: Point2,
    },
    /// Everything measured: where the arc goes, or an option.
    Place(Corner),
}

#[derive(Debug)]
pub(crate) struct DimAngularCommand {
    state: AngularState,
    options: TextOptions,
    style: DimStyleId,
}

impl DimAngularCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: AngularState::Object,
            options: TextOptions::new(),
            style: DimStyleId::FIRST,
        }
    }

    fn dimension(&self, corner: Corner, place: Point2) -> DimensionEntity {
        self.options
            .apply(DimensionEntity::new(corner.measure(place), place).in_style(self.style))
    }

    /// The point a vertex's ray goes through, or why not: it cannot be the
    /// vertex itself.
    fn ray_point(vertex: Point2, point: Point2, ctx: &mut CommandContext<'_>) -> Option<Point2> {
        if TOLERANCE.around(&[vertex, point]).coincident(vertex, point) {
            ctx.message("The points of an angle must be away from its vertex.");
            None
        } else {
            Some(point)
        }
    }
}

impl Command for DimAngularCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.current_dim_style();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        if let Some(prompt) = self.options.prompt() {
            return prompt;
        }
        match &self.state {
            AngularState::Object => {
                Prompt::new("Select arc, circle, line").with_default("specify vertex")
            }
            AngularState::SecondEndpoint { .. } | AngularState::Second { .. } => {
                Prompt::new("Specify second angle endpoint")
            }
            AngularState::SecondLine { .. } => Prompt::new("Select second line"),
            AngularState::Vertex => Prompt::new("Specify angle vertex"),
            AngularState::First { .. } => Prompt::new("Specify first angle endpoint"),
            AngularState::Place(_) => Prompt::new("Specify dimension arc line location"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        if let Some(expected) = self.options.expected() {
            return expected;
        }
        match &self.state {
            AngularState::Object | AngularState::SecondLine { .. } => ExpectedInput::object(),
            AngularState::SecondEndpoint { .. }
            | AngularState::Vertex
            | AngularState::First { .. }
            | AngularState::Second { .. } => ExpectedInput::point(),
            AngularState::Place(_) => ExpectedInput::point().with_keywords(&OPTIONS),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        if self.options.step(&input) {
            return Step::Continue;
        }
        match (&self.state, input) {
            (AngularState::Object, CommandInput::Pick { entity, at }) => {
                match ctx.drawing.entity(entity) {
                    Some(Entity::Arc(arc)) => {
                        let arc = arc.arc();
                        self.state = AngularState::Place(Corner::Points {
                            vertex: arc.center,
                            a: arc.start_point(),
                            b: arc.end_point(),
                        });
                    }
                    Some(Entity::Circle(circle)) => {
                        let vertex = circle.center;
                        if let Some(a) = Self::ray_point(vertex, at, ctx) {
                            self.state = AngularState::SecondEndpoint { vertex, a };
                        }
                    }
                    Some(Entity::Line(line)) => {
                        self.state = AngularState::SecondLine {
                            first: line.segment(),
                            at,
                        };
                    }
                    Some(other) => ctx.message(format!(
                        "A {} has no angle to dimension: pick an arc, a circle, or a line.",
                        other.kind_name()
                    )),
                    None => {}
                }
            }
            (AngularState::Object, CommandInput::Accept) => self.state = AngularState::Vertex,
            (AngularState::SecondEndpoint { vertex, a }, CommandInput::Point(b)) => {
                let (vertex, a) = (*vertex, *a);
                if let Some(b) = Self::ray_point(vertex, b, ctx) {
                    self.state = AngularState::Place(Corner::Points { vertex, a, b });
                }
            }
            (
                AngularState::SecondLine { first, at },
                CommandInput::Pick {
                    entity,
                    at: second_at,
                },
            ) => {
                let Some(Entity::Line(second)) = ctx.drawing.entity(entity) else {
                    ctx.message("Pick a second line.");
                    return Step::Continue;
                };
                let second = second.segment();
                let Some(vertex) = line_line(first, &second) else {
                    ctx.message("The lines are parallel: they make no angle.");
                    return Step::Continue;
                };
                // The rays go through the picks, or, for a pick on the
                // vertex, through the line's farther end.
                let farther = |segment: &Segment| {
                    if vertex.distance_to(segment.start) >= vertex.distance_to(segment.end) {
                        segment.start
                    } else {
                        segment.end
                    }
                };
                let away = |segment: &Segment, pick: Point2| {
                    if TOLERANCE.around(&[vertex, pick]).coincident(vertex, pick) {
                        farther(segment)
                    } else {
                        pick
                    }
                };
                let (a, b) = (away(first, *at), away(&second, second_at));
                self.state = AngularState::Place(Corner::Lines { vertex, a, b });
            }
            (AngularState::Vertex, CommandInput::Point(vertex)) => {
                self.state = AngularState::First { vertex };
            }
            (AngularState::First { vertex }, CommandInput::Point(a)) => {
                let vertex = *vertex;
                if let Some(a) = Self::ray_point(vertex, a, ctx) {
                    self.state = AngularState::Second { vertex, a };
                }
            }
            (AngularState::Second { vertex, a }, CommandInput::Point(b)) => {
                let (vertex, a) = (*vertex, *a);
                if let Some(b) = Self::ray_point(vertex, b, ctx) {
                    self.state = AngularState::Place(Corner::Points { vertex, a, b });
                }
            }
            (AngularState::Place(corner), CommandInput::Point(place)) => {
                return commit(self.dimension(*corner, place), ctx);
            }
            (AngularState::Place(corner), CommandInput::Keyword(keyword)) => {
                let corner = *corner;
                // Measured on the side of the smaller angle — between the
                // rays — for want of the pointer.
                let (vertex, a, b) = match corner {
                    Corner::Points { vertex, a, b } | Corner::Lines { vertex, a, b } => {
                        (vertex, a, b)
                    }
                };
                let between = (a - vertex) + (b - vertex);
                let place = if between.length() > 1e-9 * vertex.distance_to(a) {
                    vertex + between
                } else {
                    vertex + perpendicular(a - vertex)
                };
                let measured = TextOptions::measured(&self.dimension(corner, place), ctx);
                self.options.open(keyword, || measured);
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        match &self.state {
            AngularState::First { vertex } | AngularState::Second { vertex, .. } => {
                out.push(OverlayItem::PreviewSegment {
                    start: *vertex,
                    end: cursor,
                });
            }
            AngularState::Place(corner) => {
                let dimension = self.dimension(*corner, cursor);
                if dimension.check().is_ok() {
                    out.push(OverlayItem::PreviewEntity(dimension.into()));
                }
            }
            _ => {}
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            AngularState::First { vertex } | AngularState::Second { vertex, .. } => Some(*vertex),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum RadialState {
    Object,
    Place { center: Point2, radius: f64 },
}

/// DIMRADIUS, or DIMDIAMETER when `diameter`.
#[derive(Debug)]
pub(crate) struct DimRadialCommand {
    diameter: bool,
    state: RadialState,
    options: TextOptions,
    style: DimStyleId,
}

impl DimRadialCommand {
    pub(crate) fn radius() -> Self {
        Self::with(false)
    }

    pub(crate) fn diameter() -> Self {
        Self::with(true)
    }

    fn with(diameter: bool) -> Self {
        Self {
            diameter,
            state: RadialState::Object,
            options: TextOptions::new(),
            style: DimStyleId::FIRST,
        }
    }

    fn dimension(&self, center: Point2, radius: f64, place: Point2) -> DimensionEntity {
        let measure = if self.diameter {
            Measure::Diameter { center, radius }
        } else {
            Measure::Radius { center, radius }
        };
        self.options
            .apply(DimensionEntity::new(measure, place).in_style(self.style))
    }
}

/// The circle a picked object lies on, or what it is.
fn circle_of(entity: &Entity) -> Result<(Point2, f64), &'static str> {
    match entity {
        Entity::Circle(circle) => Ok((circle.center, circle.radius)),
        Entity::Arc(arc) => Ok((arc.center, arc.radius)),
        other => Err(other.kind_name()),
    }
}

impl Command for DimRadialCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.current_dim_style();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        if let Some(prompt) = self.options.prompt() {
            return prompt;
        }
        match &self.state {
            RadialState::Object => Prompt::new("Select arc or circle"),
            RadialState::Place { .. } => Prompt::new("Specify dimension line location"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        if let Some(expected) = self.options.expected() {
            return expected;
        }
        match &self.state {
            RadialState::Object => ExpectedInput::object(),
            RadialState::Place { .. } => ExpectedInput::point().with_keywords(&OPTIONS),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        if self.options.step(&input) {
            return Step::Continue;
        }
        match (&self.state, input) {
            (RadialState::Object, CommandInput::Pick { entity, .. }) => {
                match ctx.drawing.entity(entity).map(circle_of) {
                    Some(Ok((center, radius))) => {
                        self.state = RadialState::Place { center, radius };
                    }
                    Some(Err(kind)) => ctx.message(format!(
                        "A {kind} has no radius to dimension: pick an arc or a circle."
                    )),
                    None => {}
                }
            }
            (RadialState::Place { center, radius }, CommandInput::Point(place)) => {
                return commit(self.dimension(*center, *radius, place), ctx);
            }
            (RadialState::Place { center, radius }, CommandInput::Keyword(keyword)) => {
                let measured = TextOptions::measured(
                    &self.dimension(
                        *center,
                        *radius,
                        *center + cad_core::Vec2::new(*radius, 0.0),
                    ),
                    ctx,
                );
                self.options.open(keyword, || measured);
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let RadialState::Place { center, radius } = &self.state {
            let dimension = self.dimension(*center, *radius, cursor);
            if dimension.check().is_ok() {
                out.push(OverlayItem::PreviewEntity(dimension.into()));
            }
        }
    }
}
