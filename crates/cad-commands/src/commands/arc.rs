//! ARC: through three points, or by start, centre, and end or included angle.

use cad_core::Point2;
use cad_core::tolerance::{AngularTolerance, LinearTolerance};
use cad_document::{ArcEntity, Entity};
use cad_geometry::Arc;
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const CENTER: Keyword = Keyword::new("Center");
const ANGLE: Keyword = Keyword::new("Angle");
const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

#[derive(Debug)]
enum State {
    Start,
    /// A second point on the arc, or `Center` to switch method.
    Second {
        start: Point2,
    },
    /// The end point of a three-point arc.
    End {
        start: Point2,
        second: Point2,
    },
    Center {
        start: Point2,
    },
    /// The end direction as seen from the centre, or `Angle`.
    EndFromCenter {
        start: Point2,
        center: Point2,
    },
    IncludedAngle {
        start: Point2,
        center: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct ArcCommand {
    state: State,
}

impl ArcCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Start,
        }
    }

    /// The arc the current state would produce if its next input were `cursor`.
    fn candidate(&self, cursor: Point2) -> Option<Arc> {
        match &self.state {
            State::End { start, second } => Arc::through_points(*start, *second, cursor, TOLERANCE),
            State::EndFromCenter { start, center } => {
                from_center(*start, *center, direction(*center, cursor)?)
            }
            State::Start
            | State::Second { .. }
            | State::Center { .. }
            | State::IncludedAngle { .. } => None,
        }
    }
}

/// The angle of `to` as seen from `from`, or `None` when they coincide.
fn direction(from: Point2, to: Point2) -> Option<f64> {
    let offset = to - from;
    (!TOLERANCE.is_zero(offset.length())).then(|| offset.y.atan2(offset.x))
}

/// The arc around `center` from `start`, counter-clockwise to `end_angle`.
fn from_center(start: Point2, center: Point2, end_angle: f64) -> Option<Arc> {
    let start_angle = direction(center, start)?;
    Some(Arc::from_angles(
        center,
        center.distance_to(start),
        start_angle,
        end_angle,
    ))
}

/// The arc around `center` from `start` through `degrees`: counter-clockwise
/// when positive, clockwise when negative.
fn by_included_angle(start: Point2, center: Point2, degrees: f64) -> Option<Arc> {
    let start_angle = direction(center, start)?;
    let sweep = degrees.to_radians();
    let turns = sweep.abs() / std::f64::consts::TAU;
    if AngularTolerance::DEFAULT.is_zero(sweep) || turns >= 1.0 || !sweep.is_finite() {
        return None;
    }
    let radius = center.distance_to(start);
    // Stored counter-clockwise: a clockwise arc starts where it would have ended.
    Some(if sweep > 0.0 {
        Arc::new(center, radius, start_angle, sweep)
    } else {
        Arc::new(center, radius, start_angle + sweep, -sweep)
    })
}

fn create(arc: Option<Arc>, problem: &str, ctx: &mut CommandContext<'_>) -> Step {
    let Some(arc) = arc.filter(|arc| {
        !TOLERANCE.is_zero(arc.radius) && !AngularTolerance::DEFAULT.is_zero(arc.sweep)
    }) else {
        ctx.message(problem);
        return Step::Continue;
    };
    let mut transaction = ctx.transaction();
    transaction
        .insert(ArcEntity::from(arc))
        .expect("an arc has no text style to miss");
    transaction.commit();
    Step::Finished
}

impl Command for ArcCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match &self.state {
            State::Start => "Specify start point of arc",
            State::Second { .. } => "Specify second point of arc",
            State::End { .. } | State::EndFromCenter { .. } => "Specify end point of arc",
            State::Center { .. } => "Specify center point of arc",
            State::IncludedAngle { .. } => "Specify included angle",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Start | State::End { .. } | State::Center { .. } => ExpectedInput::point(),
            State::Second { .. } => ExpectedInput::point().with_keywords(&[CENTER]),
            State::EndFromCenter { .. } => ExpectedInput::point().with_keywords(&[ANGLE]),
            // A typed number only: there is no base point, so a point is refused.
            State::IncludedAngle { .. } => ExpectedInput::new(InputKind::Angle),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (_, CommandInput::Accept) => return Step::Finished,

            (State::Start, CommandInput::Point(start)) => self.state = State::Second { start },
            (State::Second { start }, CommandInput::Point(second)) => {
                self.state = State::End {
                    start: *start,
                    second,
                };
            }
            (State::Second { start }, CommandInput::Keyword(keyword)) if keyword == CENTER => {
                self.state = State::Center { start: *start };
            }
            (State::End { start, second }, CommandInput::Point(end)) => {
                let arc = Arc::through_points(*start, *second, end, TOLERANCE);
                return create(
                    arc,
                    "The three points lie on a line; no arc passes through them.",
                    ctx,
                );
            }

            (State::Center { start }, CommandInput::Point(center)) => {
                if TOLERANCE.coincident(*start, center) {
                    ctx.message("The center cannot be the start point.");
                } else {
                    self.state = State::EndFromCenter {
                        start: *start,
                        center,
                    };
                }
            }
            (State::EndFromCenter { start, center }, CommandInput::Point(end)) => {
                let arc =
                    direction(*center, end).and_then(|angle| from_center(*start, *center, angle));
                return create(arc, "The end point cannot be the center.", ctx);
            }
            (State::EndFromCenter { start, center }, CommandInput::Keyword(keyword))
                if keyword == ANGLE =>
            {
                self.state = State::IncludedAngle {
                    start: *start,
                    center: *center,
                };
            }
            (State::IncludedAngle { start, center }, CommandInput::Angle(degrees)) => {
                let arc = by_included_angle(*start, *center, degrees);
                return create(
                    arc,
                    "The included angle must be between -360 and 360, and not zero.",
                    ctx,
                );
            }

            // Input the current state did not ask for.
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(anchor) = self.base_point() {
            out.push(OverlayItem::PreviewSegment {
                start: anchor,
                end: cursor,
            });
        }
        if let Some(arc) = self.candidate(cursor) {
            out.push(OverlayItem::PreviewEntity(Entity::Arc(ArcEntity::from(
                arc,
            ))));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Second { start } | State::Center { start } => Some(*start),
            State::End { second, .. } => Some(*second),
            State::EndFromCenter { center, .. } => Some(*center),
            State::Start | State::IncludedAngle { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use cad_core::Point2;
    use cad_document::{ArcEntity, Entity};

    use crate::OverlayItem;
    use crate::testing::Harness;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn arcs(h: &Harness) -> Vec<ArcEntity> {
        h.entities()
            .into_iter()
            .filter_map(|entity| match entity {
                Entity::Arc(arc) => Some(arc),
                _ => None,
            })
            .collect()
    }

    /// Centre, radius, start angle and sweep, all within rounding.
    fn assert_arc(arc: &ArcEntity, center: Point2, radius: f64, start: f64, sweep: f64) {
        let geometry = arc.arc();
        assert!(
            geometry.center.distance_to(center) < 1e-9,
            "center {:?}",
            geometry.center
        );
        assert!(
            (geometry.radius - radius).abs() < 1e-9,
            "radius {}",
            geometry.radius
        );
        assert!(
            (geometry.start - start).abs() < 1e-9,
            "start {}",
            geometry.start
        );
        assert!(
            (geometry.sweep - sweep).abs() < 1e-9,
            "sweep {}",
            geometry.sweep
        );
    }

    #[test]
    fn three_points_counter_clockwise() {
        let mut h = Harness::new();
        h.enter("ARC");
        assert_eq!(h.prompt(), "Specify start point of arc:");
        h.enter("10,0");
        assert_eq!(h.prompt(), "Specify second point of arc or [Center]:");
        h.enter("0,10");
        assert_eq!(h.prompt(), "Specify end point of arc:");
        h.enter("-10,0");
        assert_eq!(h.prompt(), "Command:");
        assert_arc(&arcs(&h)[0], p(0.0, 0.0), 10.0, 0.0, PI);
    }

    #[test]
    fn three_points_clockwise_are_stored_counter_clockwise() {
        let mut h = Harness::new();
        h.enter_all(&["A", "10,0", "0,-10", "-10,0"]);
        // Through the south: counter-clockwise it runs from west (π) to east.
        assert_arc(&arcs(&h)[0], p(0.0, 0.0), 10.0, PI, PI);
    }

    #[test]
    fn collinear_points_are_refused_and_the_end_can_be_given_again() {
        let mut h = Harness::new();
        h.enter_all(&["A", "0,0", "5,0", "10,0"]);
        assert_eq!(
            h.log().last().unwrap(),
            "The three points lie on a line; no arc passes through them."
        );
        assert_eq!(h.prompt(), "Specify end point of arc:");
        h.enter("10,5");
        assert_eq!(arcs(&h).len(), 1);
    }

    #[test]
    fn start_center_end_runs_counter_clockwise_from_the_start() {
        let mut h = Harness::new();
        h.enter_all(&["A", "10,0", "C"]);
        assert_eq!(h.prompt(), "Specify center point of arc:");
        h.enter("0,0");
        assert_eq!(h.prompt(), "Specify end point of arc or [Angle]:");
        // The end point only gives a direction: its distance does not matter.
        h.enter("0,500");
        assert_arc(&arcs(&h)[0], p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2);
    }

    #[test]
    fn start_center_angle_positive_and_negative() {
        let mut h = Harness::new();
        h.enter_all(&["A", "10,0", "C", "0,0", "A"]);
        assert_eq!(h.prompt(), "Specify included angle:");
        h.enter("90");
        assert_arc(&arcs(&h)[0], p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2);

        // Negative: clockwise from the start, stored from where it ends.
        h.enter_all(&["A", "10,0", "C", "0,0", "A", "-90"]);
        assert_arc(&arcs(&h)[1], p(0.0, 0.0), 10.0, 1.5 * PI, FRAC_PI_2);
    }

    #[test]
    fn impossible_included_angles_are_refused() {
        let mut h = Harness::new();
        h.enter_all(&["A", "10,0", "C", "0,0", "A"]);
        for angle in ["0", "360", "-400"] {
            h.enter(angle);
            assert_eq!(
                h.log().last().unwrap(),
                "The included angle must be between -360 and 360, and not zero.",
                "{angle}"
            );
        }
        // A point makes no sense here: there is nothing to measure it from.
        h.enter("5,5");
        assert_eq!(h.log().last().unwrap(), "A number is required.");
        assert_eq!(h.prompt(), "Specify included angle:");
        assert!(arcs(&h).is_empty());
    }

    #[test]
    fn degenerate_centres_and_ends_are_refused() {
        let mut h = Harness::new();
        h.enter_all(&["A", "10,0", "C", "10,0"]);
        assert_eq!(
            h.log().last().unwrap(),
            "The center cannot be the start point."
        );
        h.enter_all(&["0,0", "0,0"]);
        assert_eq!(
            h.log().last().unwrap(),
            "The end point cannot be the center."
        );
        assert!(arcs(&h).is_empty());
    }

    #[test]
    fn the_preview_follows_the_cursor_through_the_third_point() {
        let mut h = Harness::new();
        h.enter_all(&["A", "10,0", "0,10"]);
        h.move_to(390.0, 300.0); // world (-10, 0)
        let overlay = h.session.overlay();
        let previewed = overlay.iter().find_map(|item| match item {
            OverlayItem::PreviewEntity(Entity::Arc(arc)) => Some(*arc),
            _ => None,
        });
        assert_arc(
            &previewed.expect("an arc preview"),
            p(0.0, 0.0),
            10.0,
            0.0,
            PI,
        );
        assert!(h.entities().is_empty(), "a preview is not an entity");
    }

    #[test]
    fn an_arc_is_one_undo_step_and_is_picked_only_on_its_sweep() {
        let mut h = Harness::new();
        h.enter_all(&["A", "50,0", "0,50", "-50,0"]);
        h.click_world(p(0.0, -50.0)); // on the circle, but not on the arc
        assert!(h.session.selection().is_empty());
        h.escape(); // the click on empty space opened a selection box
        h.click_world(p(0.0, 50.0));
        assert_eq!(h.session.selection().len(), 1);

        h.undo();
        assert!(h.entities().is_empty());
    }
}
