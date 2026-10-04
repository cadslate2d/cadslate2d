//! LINE: a chain of straight segments.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::LineEntity;
use cad_geometry::Segment;
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, OverlayItem, Step};

#[derive(Debug)]
enum LineState {
    AwaitFirstPoint,
    AwaitNextPoint { previous: Point2 },
}

/// ```text
/// AwaitFirstPoint ──point──▶ AwaitNextPoint ──point──▶ AwaitNextPoint ──accept──▶ done
/// ```
///
/// Each placed segment is committed as its own transaction; the segment that
/// follows the cursor is a transient preview and never enters the document.
#[derive(Debug)]
pub(crate) struct LineCommand {
    state: LineState,
}

impl LineCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: LineState::AwaitFirstPoint,
        }
    }
}

impl Command for LineCommand {
    fn prompt(&self) -> String {
        match self.state {
            LineState::AwaitFirstPoint => "Specify first point:",
            LineState::AwaitNextPoint { .. } => "Specify next point:",
        }
        .to_owned()
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::Point
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let CommandInput::Point(point) = input else {
            return Step::Finished;
        };
        match &mut self.state {
            LineState::AwaitFirstPoint => {
                self.state = LineState::AwaitNextPoint { previous: point };
            }
            LineState::AwaitNextPoint { previous } => {
                if Segment::new(*previous, point).is_degenerate(LinearTolerance::DEFAULT) {
                    ctx.message("Zero-length line ignored.");
                } else {
                    let mut transaction = ctx.drawing.transaction();
                    transaction.insert(LineEntity::new(*previous, point));
                    transaction.commit();
                    *previous = point;
                }
            }
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let LineState::AwaitNextPoint { previous } = self.state {
            out.push(OverlayItem::PreviewSegment {
                start: previous,
                end: cursor,
            });
        }
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
    fn prompts_follow_the_state_machine() {
        let mut h = Harness::new();
        h.enter("LINE");
        assert_eq!(h.prompt(), "Specify first point:");
        h.enter("0,0");
        assert_eq!(h.prompt(), "Specify next point:");
        h.enter("10,0");
        assert_eq!(h.prompt(), "Specify next point:");
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
        assert_eq!(h.prompt(), "Specify next point:");

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
        assert!(
            !h.session
                .overlay()
                .iter()
                .any(|item| matches!(item, OverlayItem::PreviewSegment { .. }))
        );
    }

    #[test]
    fn each_segment_is_its_own_undo_step() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", "10,10", ""]);
        h.undo();
        assert_eq!(h.lines(), vec![LineEntity::new(p(0.0, 0.0), p(10.0, 0.0))]);
        h.undo();
        assert!(h.lines().is_empty());
    }

    #[test]
    fn the_rubber_band_runs_from_the_previous_point_to_the_cursor() {
        let mut h = Harness::new();
        h.enter("L");
        h.move_to(450.0, 300.0);
        assert!(
            !h.session
                .overlay()
                .iter()
                .any(|item| matches!(item, OverlayItem::PreviewSegment { .. })),
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
        assert!(
            !h.session
                .overlay()
                .iter()
                .any(|item| matches!(item, OverlayItem::PreviewSegment { .. }))
        );
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
