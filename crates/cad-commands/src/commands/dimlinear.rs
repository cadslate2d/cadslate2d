//! DIMLINEAR and DIMALIGNED (DESIGN §34; ADR-0020): a horizontal,
//! vertical, or rotated distance, and the distance along two points.
//!
//! Two extension-line origins — or Enter and a picked line or arc, whose
//! ends they are — then where the dimension line goes, previewed at the
//! pointer as the dimension the click would make. DIMLINEAR is
//! horizontal or vertical by where the pointer is, or takes `Horizontal`,
//! `Vertical`, or `Rotated` and an angle; `Text` puts the drafter's words
//! in place of the value; `Angle` turns the text. One dimension, one
//! transaction, on the current layer in the current dimension style.

use std::f64::consts::FRAC_PI_2;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{DimStyleId, DimensionEntity, Entity, Measure};
use cad_input::{ExpectedInput, Keyword};

use super::dimtext::{ANGLE, OPTIONS as ALIGNED_OPTIONS, TEXT, TextOptions, commit};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const HORIZONTAL: Keyword = Keyword::new("Horizontal");
const VERTICAL: Keyword = Keyword::new("Vertical");
const ROTATED: Keyword = Keyword::new("Rotated");
const LINEAR_OPTIONS: [Keyword; 5] = [HORIZONTAL, VERTICAL, ROTATED, TEXT, ANGLE];

/// How the dimension line lies.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Direction {
    /// Horizontal or vertical, whichever the placement asks for.
    Auto,
    /// At this angle, radians from +X.
    Rotated(f64),
    /// Along the two points.
    Aligned,
}

#[derive(Debug)]
enum State {
    First,
    /// Enter at the first point: a line or arc whose ends are the points.
    Object,
    Second {
        a: Point2,
    },
    /// Both points given; where the dimension line goes, or an option.
    Place {
        a: Point2,
        b: Point2,
    },
    /// `Rotated`: the angle of the dimension line.
    Rotation {
        a: Point2,
        b: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct DimLinearCommand {
    state: State,
    direction: Direction,
    options: TextOptions,
    /// The style new dimensions take, as the command found it.
    style: DimStyleId,
}

impl DimLinearCommand {
    /// DIMLINEAR.
    pub(crate) fn linear() -> Self {
        Self::with(Direction::Auto)
    }

    /// DIMALIGNED.
    pub(crate) fn aligned() -> Self {
        Self::with(Direction::Aligned)
    }

    fn with(direction: Direction) -> Self {
        Self {
            state: State::First,
            direction,
            options: TextOptions::new(),
            style: DimStyleId::FIRST,
        }
    }

    fn is_aligned(&self) -> bool {
        matches!(self.direction, Direction::Aligned)
    }

    /// The dimension a click at `place` would make: what the preview shows
    /// and the click commits.
    fn dimension(&self, a: Point2, b: Point2, place: Point2) -> DimensionEntity {
        let measure = match self.direction {
            Direction::Aligned => Measure::Aligned { a, b },
            Direction::Rotated(rotation) => Measure::Linear { a, b, rotation },
            Direction::Auto => Measure::Linear {
                a,
                b,
                rotation: auto_rotation(a, b, place),
            },
        };
        self.options
            .apply(DimensionEntity::new(measure, place).in_style(self.style))
    }

    fn keywords(&self) -> &'static [Keyword] {
        if self.is_aligned() {
            &ALIGNED_OPTIONS
        } else {
            &LINEAR_OPTIONS
        }
    }
}

/// Horizontal when the placement is above or below the two points,
/// vertical when it is beside them: whichever band it is further into,
/// and horizontal on a tie.
fn auto_rotation(a: Point2, b: Point2, place: Point2) -> f64 {
    let beyond =
        |low: f64, high: f64, at: f64| (low.min(high) - at).max(at - low.max(high)).max(0.0);
    let out_x = beyond(a.x, b.x, place.x);
    let out_y = beyond(a.y, b.y, place.y);
    if out_y >= out_x { 0.0 } else { FRAC_PI_2 }
}

/// The two ends of a picked object, or what it is.
fn ends(entity: &Entity) -> Result<(Point2, Point2), &'static str> {
    match entity {
        Entity::Line(line) => Ok((line.start, line.end)),
        Entity::Arc(arc) => {
            let arc = arc.arc();
            Ok((arc.start_point(), arc.end_point()))
        }
        other => Err(other.kind_name()),
    }
}

impl Command for DimLinearCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.current_dim_style();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        if let Some(prompt) = self.options.prompt() {
            return prompt;
        }
        match &self.state {
            State::First => {
                Prompt::new("Specify first extension line origin").with_default("select object")
            }
            State::Object => Prompt::new("Select object to dimension"),
            State::Second { .. } => Prompt::new("Specify second extension line origin"),
            State::Place { .. } => Prompt::new("Specify dimension line location"),
            State::Rotation { .. } => {
                Prompt::new("Specify angle of dimension line").with_default("0")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        if let Some(expected) = self.options.expected() {
            return expected;
        }
        match &self.state {
            State::First => ExpectedInput::point(),
            State::Object => ExpectedInput::object(),
            State::Second { .. } => ExpectedInput::point().with_direct_distance(),
            State::Place { .. } => ExpectedInput::point().with_keywords(self.keywords()),
            State::Rotation { .. } => ExpectedInput::angle(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        if self.options.step(&input) {
            return Step::Continue;
        }
        match (&self.state, input) {
            (State::First, CommandInput::Point(a)) => self.state = State::Second { a },
            (State::First, CommandInput::Accept) => self.state = State::Object,
            (State::Object, CommandInput::Pick { entity, .. }) => {
                match ctx.drawing.entity(entity).map(ends) {
                    Some(Ok((a, b))) => self.state = State::Place { a, b },
                    Some(Err(kind)) => ctx.message(format!(
                        "A {kind} has no ends to dimension: pick a line or an arc."
                    )),
                    None => {}
                }
            }
            (State::Second { a }, CommandInput::Point(b)) => {
                let a = *a;
                if LinearTolerance::DEFAULT.around(&[a, b]).coincident(a, b) {
                    ctx.message("The two points of a dimension must be apart.");
                } else {
                    self.state = State::Place { a, b };
                }
            }
            (State::Place { a, b }, CommandInput::Point(place)) => {
                return commit(self.dimension(*a, *b, place), ctx);
            }
            (State::Place { a, b }, CommandInput::Keyword(keyword)) => {
                let (a, b) = (*a, *b);
                match keyword {
                    HORIZONTAL => self.direction = Direction::Rotated(0.0),
                    VERTICAL => self.direction = Direction::Rotated(FRAC_PI_2),
                    ROTATED => self.state = State::Rotation { a, b },
                    other => {
                        // Measured as the dimension stands: horizontal
                        // until placed.
                        let measured =
                            TextOptions::measured(&self.dimension(a, b, a + (b - a) * 0.5), ctx);
                        self.options.open(other, || measured);
                    }
                }
            }
            (State::Rotation { a, b }, CommandInput::Angle(degrees)) => {
                self.direction = Direction::Rotated(degrees.to_radians());
                self.state = State::Place { a: *a, b: *b };
            }
            (State::Rotation { a, b }, CommandInput::Accept) => {
                self.direction = Direction::Rotated(0.0);
                self.state = State::Place { a: *a, b: *b };
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        match &self.state {
            State::Second { a } => out.push(OverlayItem::PreviewSegment {
                start: *a,
                end: cursor,
            }),
            State::Place { a, b } => {
                let dimension = self.dimension(*a, *b, cursor);
                if dimension.check().is_ok() {
                    out.push(OverlayItem::PreviewEntity(dimension.into()));
                }
            }
            _ => {}
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Second { a } => Some(*a),
            _ => None,
        }
    }
}
