//! ELLIPSE (#181): by the ends of one axis and the half-length of the
//! other — typed, or a point that far from the centre — or by the centre
//! and one axis end; `Arc` for an arc of one, by its start and end
//! parameters, typed in degrees or given by points. With the isometric
//! snap style (#336), `Isocircle`: a circle of the current isometric plane
//! by its centre and radius or diameter, drawn as the ellipse it is.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};
use cad_document::EllipseEntity;
use cad_geometry::Ellipse;
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const ARC: Keyword = Keyword::new("Arc");
const CENTER: Keyword = Keyword::new("Center");
const ISOCIRCLE: Keyword = Keyword::new("Isocircle");
const DIAMETER: Keyword = Keyword::new("Diameter");
const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

#[derive(Debug)]
enum State {
    /// One end of an axis, or `Arc`, or `Center`.
    Start,
    /// The other end of that axis.
    OtherEnd {
        first: Point2,
    },
    /// The centre, for the `Center` method.
    Center,
    /// One end of an axis from a known centre.
    AxisEnd {
        center: Point2,
    },
    /// The half-length of the other axis, as a distance or a point.
    OtherAxis {
        center: Point2,
        major: Vec2,
    },
    /// `Arc`: the start parameter, as an angle or a point.
    ArcStart {
        ellipse: Ellipse,
    },
    /// `Arc`: the end parameter.
    ArcEnd {
        ellipse: Ellipse,
        start: f64,
    },
    /// `Isocircle`: its centre, then its radius or diameter.
    IsoCenter,
    IsoRadius {
        center: Point2,
    },
    IsoDiameter {
        center: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct EllipseCommand {
    state: State,
    arc: bool,
    /// The isometric plane, when the snap style is isometric.
    isoplane: Option<cad_core::iso::Plane>,
}

impl EllipseCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Start,
            arc: false,
            isoplane: None,
        }
    }

    /// The isocircle of `radius` round `center` on the current plane.
    fn isocircle(&self, center: Point2, radius: f64) -> Option<Ellipse> {
        let plane = self.isoplane?;
        if !radius.is_finite() || TOLERANCE.is_zero(radius) {
            return None;
        }
        let (major, ratio) = plane.circle(radius);
        let ellipse = Ellipse::full(center, major, ratio);
        ellipse.is_valid().then_some(ellipse)
    }

    /// The whole ellipse `other` away from the axis through `center` along
    /// `major`; when that is the longer, the axes trade places, so the
    /// major axis is always the long one.
    fn shaped(center: Point2, major: Vec2, other: f64) -> Option<Ellipse> {
        let length = major.length();
        if TOLERANCE.is_zero(length) || !other.is_finite() || TOLERANCE.is_zero(other) {
            return None;
        }
        let ellipse = if other <= length {
            Ellipse::full(center, major, other / length)
        } else {
            let across = Vec2::new(-major.y, major.x) * (other / length);
            Ellipse::full(center, across, length / other)
        };
        ellipse.is_valid().then_some(ellipse)
    }

    /// The ellipse the current state would make if its next input were
    /// `cursor`.
    fn candidate(&self, cursor: Point2) -> Option<Ellipse> {
        match &self.state {
            State::OtherAxis { center, major } => {
                Self::shaped(*center, *major, cursor.distance_to(*center))
            }
            State::ArcStart { ellipse } => Some(*ellipse),
            State::IsoRadius { center } => self.isocircle(*center, cursor.distance_to(*center)),
            State::IsoDiameter { center } => {
                self.isocircle(*center, cursor.distance_to(*center) / 2.0)
            }
            State::ArcEnd { ellipse, start } => {
                let end = ellipse.parameter_of(cursor);
                Some(Ellipse::from_parameters(
                    ellipse.center,
                    ellipse.major,
                    ellipse.ratio,
                    *start,
                    end,
                ))
            }
            _ => None,
        }
    }
}

fn create(ellipse: Ellipse, ctx: &mut CommandContext<'_>) -> Step {
    let mut transaction = ctx.transaction();
    transaction
        .insert(EllipseEntity::from(ellipse))
        .expect("an ellipse has no text style to miss");
    transaction.commit();
    Step::Finished
}

impl Command for EllipseCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.isoplane = ctx.settings.isometric.then_some(ctx.settings.isoplane);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(match &self.state {
            State::Start => "Specify axis endpoint of ellipse",
            State::OtherEnd { .. } => "Specify other endpoint of axis",
            State::Center => "Specify center of ellipse",
            State::AxisEnd { .. } => "Specify endpoint of axis",
            State::OtherAxis { .. } => "Specify distance to other axis",
            State::ArcStart { .. } => "Specify start parameter",
            State::ArcEnd { .. } => "Specify end parameter",
            State::IsoCenter => "Specify center of isocircle",
            State::IsoRadius { .. } => "Specify radius of isocircle",
            State::IsoDiameter { .. } => "Specify diameter of isocircle",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Start if self.isoplane.is_some() => {
                ExpectedInput::point().with_keywords(&[ARC, CENTER, ISOCIRCLE])
            }
            State::Start => ExpectedInput::point().with_keywords(&[ARC, CENTER]),
            State::OtherEnd { .. } | State::Center | State::AxisEnd { .. } | State::IsoCenter => {
                ExpectedInput::point()
            }
            State::IsoRadius { .. } => ExpectedInput::distance().with_keywords(&[DIAMETER]),
            State::IsoDiameter { .. } => ExpectedInput::distance(),
            State::OtherAxis { .. } => ExpectedInput::distance(),
            State::ArcStart { .. } | State::ArcEnd { .. } => ExpectedInput::angle(),
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::OtherEnd { first } => Some(*first),
            State::AxisEnd { center } | State::OtherAxis { center, .. } => Some(*center),
            State::ArcStart { ellipse } | State::ArcEnd { ellipse, .. } => Some(ellipse.center),
            State::IsoRadius { center } | State::IsoDiameter { center } => Some(*center),
            State::Start | State::Center | State::IsoCenter => None,
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(base) = self.base_point() {
            out.push(OverlayItem::PreviewSegment {
                start: base,
                end: cursor,
            });
        }
        if let Some(ellipse) = self.candidate(cursor) {
            out.push(OverlayItem::PreviewEntity(
                EllipseEntity::from(ellipse).into(),
            ));
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Start, CommandInput::Keyword(keyword)) => {
                if keyword == ARC {
                    self.arc = true;
                } else if keyword == CENTER {
                    self.state = State::Center;
                } else if keyword == ISOCIRCLE {
                    self.state = State::IsoCenter;
                }
                Step::Continue
            }
            (State::IsoCenter, CommandInput::Point(center)) => {
                self.state = State::IsoRadius { center };
                Step::Continue
            }
            (State::IsoRadius { center }, CommandInput::Keyword(_)) => {
                self.state = State::IsoDiameter { center: *center };
                Step::Continue
            }
            (State::IsoRadius { center } | State::IsoDiameter { center }, input) => {
                let length = match input {
                    CommandInput::Distance(length) => length,
                    CommandInput::Point(point) => point.distance_to(*center),
                    _ => return Step::Continue,
                };
                let radius = if matches!(self.state, State::IsoDiameter { .. }) {
                    length / 2.0
                } else {
                    length
                };
                match self.isocircle(*center, radius) {
                    Some(ellipse) => create(ellipse, ctx),
                    None => {
                        ctx.message("The isocircle must have a radius.");
                        Step::Continue
                    }
                }
            }
            (State::Start, CommandInput::Point(first)) => {
                self.state = State::OtherEnd { first };
                Step::Continue
            }
            (State::OtherEnd { first }, CommandInput::Point(second)) => {
                if TOLERANCE.coincident(*first, second) {
                    ctx.message("The axis endpoints must be different.");
                    return Step::Continue;
                }
                let center = *first + (second - *first) * 0.5;
                self.state = State::OtherAxis {
                    center,
                    major: second - center,
                };
                Step::Continue
            }
            (State::Center, CommandInput::Point(center)) => {
                self.state = State::AxisEnd { center };
                Step::Continue
            }
            (State::AxisEnd { center }, CommandInput::Point(end)) => {
                if TOLERANCE.coincident(*center, end) {
                    ctx.message("The axis endpoint must differ from the centre.");
                    return Step::Continue;
                }
                self.state = State::OtherAxis {
                    center: *center,
                    major: end - *center,
                };
                Step::Continue
            }
            (State::OtherAxis { center, major }, input) => {
                // A point gives its distance from the centre, as the session
                // measures a point at a distance prompt from the base point.
                let other = match input {
                    CommandInput::Distance(distance) => distance,
                    CommandInput::Point(point) => point.distance_to(*center),
                    _ => return Step::Continue,
                };
                let Some(ellipse) = Self::shaped(*center, *major, other) else {
                    ctx.message("The other axis must have length.");
                    return Step::Continue;
                };
                if self.arc {
                    self.state = State::ArcStart { ellipse };
                    Step::Continue
                } else {
                    create(ellipse, ctx)
                }
            }
            (State::ArcStart { ellipse }, input) => {
                let start = match input {
                    CommandInput::Angle(degrees) => degrees.to_radians(),
                    CommandInput::Point(point) => ellipse.parameter_of(point),
                    _ => return Step::Continue,
                };
                self.state = State::ArcEnd {
                    ellipse: *ellipse,
                    start,
                };
                Step::Continue
            }
            (State::ArcEnd { ellipse, start }, input) => {
                let end = match input {
                    CommandInput::Angle(degrees) => degrees.to_radians(),
                    CommandInput::Point(point) => ellipse.parameter_of(point),
                    _ => return Step::Continue,
                };
                let arc = Ellipse::from_parameters(
                    ellipse.center,
                    ellipse.major,
                    ellipse.ratio,
                    *start,
                    end,
                );
                create(arc, ctx)
            }
            _ => Step::Continue,
        }
    }
}
