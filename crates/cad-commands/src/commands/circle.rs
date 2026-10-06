//! CIRCLE: by centre and radius, centre and diameter, two points, or three.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{CircleEntity, Entity};
use cad_geometry::Circle;
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const THREE_POINT: Keyword = Keyword::new("3P");
const TWO_POINT: Keyword = Keyword::new("2P");
const DIAMETER: Keyword = Keyword::new("Diameter");
const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

#[derive(Debug)]
enum State {
    /// Centre point, or a keyword choosing another method.
    Center,
    Radius {
        center: Point2,
    },
    Diameter {
        center: Point2,
    },
    /// The two ends of a diameter.
    TwoPoint {
        first: Option<Point2>,
    },
    /// Three points on the circle; holds the ones given so far.
    ThreePoint {
        points: Vec<Point2>,
    },
}

#[derive(Debug)]
pub(crate) struct CircleCommand {
    state: State,
}

impl CircleCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Center,
        }
    }

    /// The circle the current state would produce if its next input were
    /// `cursor`, when that is already determined.
    fn candidate(&self, cursor: Point2) -> Option<Circle> {
        match &self.state {
            State::Center | State::TwoPoint { first: None } => None,
            State::Radius { center } => Some(Circle::new(*center, center.distance_to(cursor))),
            State::Diameter { center } => {
                Some(Circle::new(*center, center.distance_to(cursor) / 2.0))
            }
            State::TwoPoint { first: Some(first) } => Some(across(*first, cursor)),
            State::ThreePoint { points } => match points.as_slice() {
                [a, b] => Circle::through_points(*a, *b, cursor, TOLERANCE),
                _ => None,
            },
        }
    }
}

/// The circle with `a`–`b` as a diameter.
fn across(a: Point2, b: Point2) -> Circle {
    let half = (b - a) * 0.5;
    Circle::new(a + half, half.length())
}

/// Commits the circle and finishes, or explains why not and keeps asking.
fn create(circle: Circle, ctx: &mut CommandContext<'_>) -> Step {
    if !circle.radius.is_finite() || TOLERANCE.is_zero(circle.radius) || circle.radius < 0.0 {
        ctx.message("The radius must be greater than zero.");
        return Step::Continue;
    }
    let mut transaction = ctx.drawing.transaction();
    transaction
        .insert(CircleEntity::new(circle.center, circle.radius))
        .expect("a circle has no text style to miss");
    transaction.commit();
    Step::Finished
}

impl Command for CircleCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match &self.state {
            State::Center => "Specify center point for circle",
            State::Radius { .. } => "Specify radius of circle",
            State::Diameter { .. } => "Specify diameter of circle",
            State::TwoPoint { first: None } => "Specify first end point of circle's diameter",
            State::TwoPoint { first: Some(_) } => "Specify second end point of circle's diameter",
            State::ThreePoint { points } => match points.len() {
                0 => "Specify first point on circle",
                1 => "Specify second point on circle",
                _ => "Specify third point on circle",
            },
        })
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Center => ExpectedInput::point().with_keywords(&[THREE_POINT, TWO_POINT]),
            State::Radius { .. } => ExpectedInput::distance().with_keywords(&[DIAMETER]),
            State::Diameter { .. } => ExpectedInput::distance(),
            State::TwoPoint { first: None } => ExpectedInput::point(),
            State::TwoPoint { first: Some(_) } => ExpectedInput::point().with_direct_distance(),
            State::ThreePoint { .. } => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&mut self.state, input) {
            (_, CommandInput::Accept) => return Step::Finished,

            (State::Center, CommandInput::Point(center)) => self.state = State::Radius { center },
            (State::Center, CommandInput::Keyword(keyword)) if keyword == THREE_POINT => {
                self.state = State::ThreePoint { points: Vec::new() };
            }
            (State::Center, CommandInput::Keyword(keyword)) if keyword == TWO_POINT => {
                self.state = State::TwoPoint { first: None };
            }

            (State::Radius { center }, CommandInput::Distance(radius)) => {
                return create(Circle::new(*center, radius), ctx);
            }
            (State::Radius { center }, CommandInput::Keyword(keyword)) if keyword == DIAMETER => {
                self.state = State::Diameter { center: *center };
            }
            (State::Diameter { center }, CommandInput::Distance(diameter)) => {
                return create(Circle::new(*center, diameter / 2.0), ctx);
            }

            (
                State::TwoPoint {
                    first: first @ None,
                },
                CommandInput::Point(point),
            ) => *first = Some(point),
            (State::TwoPoint { first: Some(first) }, CommandInput::Point(second)) => {
                return create(across(*first, second), ctx);
            }

            (State::ThreePoint { points }, CommandInput::Point(point)) => {
                if let [a, b] = points.as_slice() {
                    return match Circle::through_points(*a, *b, point, TOLERANCE) {
                        Some(circle) => create(circle, ctx),
                        None => {
                            ctx.message(
                                "The three points lie on a line; no circle passes through them.",
                            );
                            Step::Continue
                        }
                    };
                }
                points.push(point);
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
        if let Some(circle) = self.candidate(cursor).filter(|circle| circle.radius > 0.0) {
            out.push(OverlayItem::PreviewEntity(Entity::Circle(
                CircleEntity::new(circle.center, circle.radius),
            )));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Radius { center } | State::Diameter { center } => Some(*center),
            State::TwoPoint { first } => *first,
            State::ThreePoint { points } => points.last().copied(),
            State::Center => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use cad_document::{CircleEntity, Entity};

    use crate::OverlayItem;
    use crate::testing::Harness;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn circles(h: &Harness) -> Vec<CircleEntity> {
        h.entities()
            .into_iter()
            .filter_map(|entity| match entity {
                Entity::Circle(circle) => Some(circle),
                _ => None,
            })
            .collect()
    }

    fn close(a: &CircleEntity, center: Point2, radius: f64) -> bool {
        a.center.distance_to(center) < 1e-9 && (a.radius - radius).abs() < 1e-9
    }

    #[test]
    fn center_and_typed_radius() {
        let mut h = Harness::new();
        h.enter("CIRCLE");
        assert_eq!(h.prompt(), "Specify center point for circle or [3P/2P]:");
        h.enter("10,20");
        assert_eq!(h.prompt(), "Specify radius of circle or [Diameter]:");
        h.enter("5");
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(circles(&h), vec![CircleEntity::new(p(10.0, 20.0), 5.0)]);
    }

    /// DESIGN §13.3: at the idle prompt `C` is CIRCLE.
    #[test]
    fn c_starts_circle_at_the_idle_prompt() {
        let mut h = Harness::new();
        h.enter("C");
        assert_eq!(h.session.ui_state().command, Some("CIRCLE"));
    }

    #[test]
    fn the_radius_can_be_a_click_or_a_typed_point() {
        let mut clicked = Harness::new();
        clicked.enter_all(&["C", "0,0"]);
        clicked.click_world(p(3.0, 4.0));
        assert_eq!(circles(&clicked), vec![CircleEntity::new(p(0.0, 0.0), 5.0)]);

        let mut typed = Harness::new();
        typed.enter_all(&["C", "0,0", "3,4"]);
        assert_eq!(circles(&typed), circles(&clicked));

        // A relative point is measured from the centre.
        let mut relative = Harness::new();
        relative.enter_all(&["C", "100,100", "@0,7"]);
        assert_eq!(
            circles(&relative),
            vec![CircleEntity::new(p(100.0, 100.0), 7.0)]
        );
    }

    #[test]
    fn diameter_option() {
        let mut h = Harness::new();
        h.enter_all(&["C", "0,0", "D"]);
        assert_eq!(h.prompt(), "Specify diameter of circle:");
        h.enter("10");
        assert_eq!(circles(&h), vec![CircleEntity::new(p(0.0, 0.0), 5.0)]);
    }

    #[test]
    fn two_point_option_uses_the_points_as_a_diameter() {
        let mut h = Harness::new();
        h.enter_all(&["C", "2P"]);
        assert_eq!(h.prompt(), "Specify first end point of circle's diameter:");
        h.enter("0,0");
        assert_eq!(h.prompt(), "Specify second end point of circle's diameter:");
        h.enter("@10,0");
        assert_eq!(circles(&h), vec![CircleEntity::new(p(5.0, 0.0), 5.0)]);
    }

    #[test]
    fn a_bare_two_is_not_mistaken_for_the_2p_option() {
        let mut h = Harness::new();
        h.enter_all(&["C", "2"]);
        assert_eq!(
            h.log().last().unwrap(),
            "A point or an option keyword is required."
        );
        // And at the radius prompt a bare 2 is simply the radius.
        h.enter_all(&["0,0", "2"]);
        assert_eq!(circles(&h), vec![CircleEntity::new(p(0.0, 0.0), 2.0)]);
    }

    #[test]
    fn three_point_option() {
        let mut h = Harness::new();
        h.enter_all(&["C", "3P"]);
        assert_eq!(h.prompt(), "Specify first point on circle:");
        h.enter("10,0");
        assert_eq!(h.prompt(), "Specify second point on circle:");
        h.enter("0,10");
        assert_eq!(h.prompt(), "Specify third point on circle:");
        h.enter("-10,0");
        let created = circles(&h);
        assert_eq!(created.len(), 1);
        assert!(close(&created[0], p(0.0, 0.0), 10.0), "{:?}", created[0]);
    }

    #[test]
    fn collinear_points_are_refused_and_the_third_can_be_given_again() {
        let mut h = Harness::new();
        h.enter_all(&["C", "3P", "0,0", "5,0", "10,0"]);
        assert_eq!(
            h.log().last().unwrap(),
            "The three points lie on a line; no circle passes through them."
        );
        assert_eq!(h.prompt(), "Specify third point on circle:");
        assert!(circles(&h).is_empty());

        h.enter("5,5");
        assert_eq!(circles(&h).len(), 1);
    }

    #[test]
    fn a_zero_or_negative_radius_is_refused() {
        let mut h = Harness::new();
        h.enter_all(&["C", "0,0", "0"]);
        assert_eq!(
            h.log().last().unwrap(),
            "The radius must be greater than zero."
        );
        h.enter("-3");
        assert_eq!(
            h.log().last().unwrap(),
            "The radius must be greater than zero."
        );
        assert_eq!(h.prompt(), "Specify radius of circle or [Diameter]:");
        assert!(circles(&h).is_empty());

        // Clicking on the centre is a zero radius too.
        h.click_world(p(0.0, 0.0));
        assert!(circles(&h).is_empty());
    }

    #[test]
    fn enter_or_escape_abandons_the_circle() {
        let mut h = Harness::new();
        h.enter_all(&["C", "0,0", ""]);
        assert_eq!(h.prompt(), "Command:");
        h.enter_all(&["C", "0,0"]);
        h.escape();
        assert!(h.entities().is_empty());
    }

    #[test]
    fn one_undo_removes_the_circle_and_redo_restores_it() {
        let mut h = Harness::new();
        h.enter_all(&["C", "0,0", "5"]);
        let id = h.ids()[0];
        h.undo();
        assert!(h.entities().is_empty());
        h.redo();
        assert_eq!(h.ids(), vec![id]);
    }

    #[test]
    fn the_preview_shows_the_circle_and_its_radius_line() {
        let mut h = Harness::new();
        h.enter_all(&["C", "0,0"]);
        h.move_to(430.0, 260.0); // world (30, 40): radius 50
        let overlay = h.session.overlay();
        assert!(overlay.contains(&OverlayItem::PreviewSegment {
            start: p(0.0, 0.0),
            end: p(30.0, 40.0)
        }));
        assert!(overlay.contains(&OverlayItem::PreviewEntity(Entity::Circle(
            CircleEntity::new(p(0.0, 0.0), 50.0)
        ))));
        // A preview is not an entity.
        assert!(h.entities().is_empty());
    }

    #[test]
    fn a_circle_is_selected_by_clicking_its_curve_not_its_interior() {
        let mut h = Harness::new();
        h.enter_all(&["C", "0,0", "50"]);
        h.click_world(p(0.0, 0.0));
        assert!(
            h.session.selection().is_empty(),
            "the centre is not on the circle"
        );
        h.escape(); // the click on empty space opened a selection box
        h.click_world(p(50.0, 2.0));
        assert_eq!(h.session.selection().len(), 1);
    }
}
