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

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const HORIZONTAL: Keyword = Keyword::new("Horizontal");
const VERTICAL: Keyword = Keyword::new("Vertical");
const ROTATED: Keyword = Keyword::new("Rotated");
const TEXT: Keyword = Keyword::new("Text");
const ANGLE: Keyword = Keyword::new("Angle");
const LINEAR_OPTIONS: [Keyword; 5] = [HORIZONTAL, VERTICAL, ROTATED, TEXT, ANGLE];
const ALIGNED_OPTIONS: [Keyword; 2] = [TEXT, ANGLE];

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
    /// `Text`: words in place of the value, which is the default.
    Text {
        a: Point2,
        b: Point2,
        measured: String,
    },
    /// `Angle`: the text's angle.
    TextAngle {
        a: Point2,
        b: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct DimLinearCommand {
    state: State,
    direction: Direction,
    text: Option<String>,
    text_angle: Option<f64>,
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
            text: None,
            text_angle: None,
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
        DimensionEntity::new(measure, place)
            .in_style(self.style)
            .saying(self.text.clone())
            .turned(self.text_angle)
    }

    fn options(&self) -> &'static [Keyword] {
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
            State::Text { measured, .. } => {
                Prompt::new("Enter dimension text").with_default(measured.clone())
            }
            State::TextAngle { .. } => {
                Prompt::new("Specify angle of dimension text").with_default("0")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::First => ExpectedInput::point(),
            State::Object => ExpectedInput::object(),
            State::Second { .. } => ExpectedInput::point().with_direct_distance(),
            State::Place { .. } => ExpectedInput::point().with_keywords(self.options()),
            State::Rotation { .. } | State::TextAngle { .. } => ExpectedInput::angle(),
            State::Text { .. } => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
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
                let dimension = self.dimension(*a, *b, place);
                let mut transaction = ctx.drawing.transaction();
                match transaction.insert(dimension) {
                    Ok(_) => {
                        transaction.commit();
                        return Step::Finished;
                    }
                    Err(error) => {
                        drop(transaction);
                        ctx.message(error.to_string());
                    }
                }
            }
            (State::Place { a, b }, CommandInput::Keyword(keyword)) => {
                let (a, b) = (*a, *b);
                match keyword {
                    HORIZONTAL => self.direction = Direction::Rotated(0.0),
                    VERTICAL => self.direction = Direction::Rotated(FRAC_PI_2),
                    ROTATED => self.state = State::Rotation { a, b },
                    TEXT => {
                        // The default is what the text would say: measured
                        // as the dimension stands, horizontal until placed.
                        let style = ctx.drawing.tables().dim_style(self.style);
                        let measured = DimensionEntity {
                            text_override: None,
                            ..self.dimension(a, b, a + (b - a) * 0.5)
                        }
                        .content(style);
                        self.state = State::Text { a, b, measured };
                    }
                    ANGLE => self.state = State::TextAngle { a, b },
                    _ => {}
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
            (State::Text { a, b, .. }, CommandInput::Text(words)) => {
                self.text = Some(words);
                self.state = State::Place { a: *a, b: *b };
            }
            (State::Text { a, b, .. }, CommandInput::Accept) => {
                self.text = None;
                self.state = State::Place { a: *a, b: *b };
            }
            (State::TextAngle { a, b }, CommandInput::Angle(degrees)) => {
                self.text_angle = Some(degrees.to_radians());
                self.state = State::Place { a: *a, b: *b };
            }
            (State::TextAngle { a, b }, CommandInput::Accept) => {
                self.text_angle = None;
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
