//! LINE: a chain of straight segments.

use cad_core::tolerance::LinearTolerance;
use cad_core::{EntityId, Point2};
use cad_document::LineEntity;
use cad_geometry::Segment;
use cad_input::ExpectedInput;

use super::keywords::{CLOSE, UNDO};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

/// Segments needed before the chain can be closed back to its first point.
const SEGMENTS_TO_CLOSE: usize = 2;

/// ```text
/// AwaitFirstPoint ──point──▶ AwaitNextPoint ──point──▶ AwaitNextPoint ──▶ …
///                                 │  ▲                      │
///                                 │  └────── Undo ──────────┤
///                                 └── Enter / Close ──▶ done
/// ```
///
/// Each placed segment is committed immediately; the session groups the whole
/// invocation into one undo step. The segment that follows the cursor is a
/// transient preview and never enters the document.
#[derive(Debug, Default)]
pub(crate) struct LineCommand {
    /// The first point, then the end of each placed segment.
    points: Vec<Point2>,
    /// The entities created so far, in order.
    created: Vec<EntityId>,
}

impl LineCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn place(&mut self, end: Point2, ctx: &mut CommandContext<'_>) {
        let Some(&start) = self.points.last() else {
            self.points.push(end);
            return;
        };
        if Segment::new(start, end).is_degenerate(LinearTolerance::DEFAULT) {
            ctx.message("Zero-length line ignored.");
            return;
        }
        let mut transaction = ctx.drawing.transaction();
        self.created.push(
            transaction
                .insert(LineEntity::new(start, end))
                .expect("the entity has no text style to miss"),
        );
        transaction.commit();
        self.points.push(end);
    }

    /// Removes the last segment and steps the rubber band back to where it began.
    fn undo(&mut self, ctx: &mut CommandContext<'_>) {
        if self.created.pop().is_some() {
            ctx.drawing.rollback_last_in_group();
            self.points.pop();
        } else {
            ctx.message("All segments already undone.");
        }
    }
}

impl Command for LineCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(if self.points.is_empty() {
            "Specify first point"
        } else {
            "Specify next point"
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.created.len() {
            _ if self.points.is_empty() => ExpectedInput::point(),
            count if count >= SEGMENTS_TO_CLOSE => ExpectedInput::point()
                .with_direct_distance()
                .with_keywords(&[CLOSE, UNDO]),
            _ => ExpectedInput::point()
                .with_direct_distance()
                .with_keywords(&[UNDO]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Point(point) => self.place(point, ctx),
            CommandInput::Keyword(keyword) if keyword == UNDO => self.undo(ctx),
            CommandInput::Keyword(keyword) if keyword == CLOSE => {
                self.place(self.points[0], ctx);
                return Step::Finished;
            }
            CommandInput::Accept => return Step::Finished,
            CommandInput::Keyword(_)
            | CommandInput::Distance(_)
            | CommandInput::Angle(_)
            | CommandInput::Text(_)
            | CommandInput::Pick { .. } => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(&previous) = self.points.last() {
            out.push(OverlayItem::PreviewSegment {
                start: previous,
                end: cursor,
            });
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.points.last().copied()
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use cad_document::LineEntity;

    use crate::OverlayItem;
    use crate::testing::Harness;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn has_rubber_band(h: &Harness) -> bool {
        h.session
            .overlay()
            .iter()
            .any(|item| matches!(item, OverlayItem::PreviewSegment { .. }))
    }

    /// The M0 user story (DESIGN §26.1): L, 0,0, @1000<45, Enter.
    #[test]
    fn the_m0_user_story_draws_one_line() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "@1000<45", ""]);

        let lines = h.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].start, p(0.0, 0.0));
        let expected = 1000.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!(lines[0].end.distance_to(p(expected, expected)) < 1e-9);
        assert_eq!(h.prompt(), "Command:");
    }

    #[test]
    fn prompts_offer_exactly_the_options_that_apply() {
        let mut h = Harness::new();
        h.enter("LINE");
        assert_eq!(h.prompt(), "Specify first point:");
        h.enter("0,0");
        assert_eq!(h.prompt(), "Specify next point or [Undo]:");
        h.enter("10,0");
        assert_eq!(h.prompt(), "Specify next point or [Undo]:");
        h.enter("10,10");
        assert_eq!(h.prompt(), "Specify next point or [Close/Undo]:");
        h.enter("");
        assert_eq!(h.prompt(), "Command:");
    }

    #[test]
    fn successive_points_chain_segments() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "10,10", "0,10", ""]);
        assert_eq!(
            h.lines(),
            vec![
                LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)),
                LineEntity::new(p(10.0, 0.0), p(10.0, 10.0)),
                LineEntity::new(p(10.0, 10.0), p(0.0, 10.0)),
            ]
        );
    }

    #[test]
    fn finishing_after_only_the_first_point_creates_nothing() {
        let mut h = Harness::new();
        h.enter_all(&["L", "5,5", ""]);
        assert!(h.lines().is_empty());
        assert_eq!(h.prompt(), "Command:");
    }

    #[test]
    fn zero_length_segments_are_rejected_and_the_command_continues() {
        let mut h = Harness::new();
        h.enter_all(&["L", "5,5", "5,5", "@0,0"]);
        assert!(h.lines().is_empty());
        assert_eq!(h.log().last().unwrap(), "Zero-length line ignored.");
        assert_eq!(h.prompt(), "Specify next point or [Undo]:");

        h.enter_all(&["6,5", ""]);
        assert_eq!(h.lines(), vec![LineEntity::new(p(5.0, 5.0), p(6.0, 5.0))]);
    }

    #[test]
    fn escape_keeps_committed_segments_and_drops_the_pending_one() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0"]);
        h.move_to(500.0, 200.0);
        h.escape();
        assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(10.0, 0.0))]);
        assert!(!has_rubber_band(&h));
    }

    // ── Close ──

    #[test]
    fn close_returns_to_the_first_point_and_finishes() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "10,10", "C"]);
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.lines().len(), 3);
        assert_eq!(h.lines()[2], LineEntity::new(p(10.0, 10.0), p(0.0, 0.0)));
    }

    #[test]
    fn close_is_not_offered_before_two_segments_exist() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "C"]);
        assert_eq!(
            h.log().last().unwrap(),
            "A point or an option keyword is required."
        );
        assert_eq!(h.prompt(), "Specify next point or [Undo]:");
        assert_eq!(h.lines().len(), 1);
    }

    // ── Undo inside the command ──

    #[test]
    fn undo_removes_the_last_segment_and_steps_the_rubber_band_back() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "10,10"]);
        h.move_to(400.0, 300.0);

        h.enter("U");
        assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(10.0, 0.0))]);
        assert!(h.session.overlay().contains(&OverlayItem::PreviewSegment {
            start: p(10.0, 0.0),
            end: p(0.0, 0.0)
        }));

        // Drawing continues from where the undone segment began.
        h.enter_all(&["@0,5", ""]);
        assert_eq!(h.lines()[1], LineEntity::new(p(10.0, 0.0), p(10.0, 5.0)));
    }

    #[test]
    fn undo_can_take_back_every_segment_but_not_the_first_point() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "undo"]);
        assert!(h.lines().is_empty());
        assert_eq!(h.prompt(), "Specify next point or [Undo]:");

        h.enter("U");
        assert_eq!(h.log().last().unwrap(), "All segments already undone.");
        h.enter_all(&["5,5", ""]);
        assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(5.0, 5.0))]);
    }

    #[test]
    fn a_segment_undone_inside_the_command_cannot_be_redone_afterwards() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "10,10", "U", ""]);
        h.redo();
        assert_eq!(h.log().last().unwrap(), "Nothing to redo.");
        assert_eq!(h.lines().len(), 1);
    }

    // ── undo granularity: one invocation, one undo step ──

    #[test]
    fn one_undo_after_the_command_removes_everything_it_drew() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "1,0", ""]);
        h.enter_all(&["L", "0,5", "10,5", "10,10", "C"]);
        assert_eq!(h.lines().len(), 4);

        h.undo();
        assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(1.0, 0.0))]);
        h.redo();
        assert_eq!(h.lines().len(), 4);
    }

    #[test]
    fn a_cancelled_command_is_still_one_undo_step() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "10,10"]);
        h.escape();
        h.undo();
        assert!(h.lines().is_empty());
    }

    // ── direct distance ──

    #[test]
    fn a_bare_number_travels_that_far_towards_the_cursor() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        h.move_to(600.0, 300.0); // due east of the origin
        h.enter("500");
        assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(500.0, 0.0))]);

        h.move_to(500.0 + 400.0, 0.0); // straight up from (500, 0)
        h.enter_all(&["25", ""]);
        assert!(h.lines()[1].end.distance_to(p(500.0, 25.0)) < 1e-9);
    }

    #[test]
    fn direct_distance_needs_a_previous_point_and_a_direction() {
        let mut h = Harness::new();
        h.enter_all(&["L", "500"]);
        assert_eq!(h.log().last().unwrap(), "A point is required.");

        // A previous point but no cursor on the canvas: no direction to travel in.
        h.enter_all(&["0,0", "500"]);
        assert_eq!(
            h.log().last().unwrap(),
            "Point the cursor in a direction, then enter the distance."
        );
        assert!(h.lines().is_empty());

        // The cursor exactly on the previous point gives no direction either.
        h.move_to(400.0, 300.0);
        h.enter("500");
        assert_eq!(
            h.log().last().unwrap(),
            "Point the cursor in a direction, then enter the distance."
        );
        assert_eq!(h.prompt(), "Specify next point or [Undo]:");
    }

    // ── preview ──

    #[test]
    fn the_rubber_band_runs_from_the_previous_point_to_the_cursor() {
        let mut h = Harness::new();
        h.enter("L");
        h.move_to(450.0, 300.0);
        assert!(
            !has_rubber_band(&h),
            "no rubber band before the first point"
        );

        h.enter("0,0");
        assert!(h.session.overlay().contains(&OverlayItem::PreviewSegment {
            start: p(0.0, 0.0),
            end: p(50.0, 0.0)
        }));

        h.enter("20,0");
        h.move_to(400.0, 250.0);
        assert!(h.session.overlay().contains(&OverlayItem::PreviewSegment {
            start: p(20.0, 0.0),
            end: p(0.0, 50.0)
        }));

        h.enter("");
        assert!(!has_rubber_band(&h));
    }

    /// DESIGN §26.3 criterion 9: the preview is transient.
    #[test]
    fn pointer_movement_during_the_command_never_touches_the_document() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        let revision = h.session.drawing().revision();
        let next_id = h.session.drawing().next_entity_id();

        for step in 0..1000 {
            let dirty = h.move_to(f64::from(step % 800), f64::from(step % 600));
            assert!(!dirty.contains(crate::Dirty::SCENE));
        }

        assert_eq!(h.session.drawing().revision(), revision);
        assert_eq!(h.session.drawing().next_entity_id(), next_id);
        assert!(h.lines().is_empty());
    }
}
