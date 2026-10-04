//! PLINE: one polyline entity through a chain of points.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{Entity, PolylineEntity};
use cad_input::ExpectedInput;

use super::keywords::{CLOSE, UNDO};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

/// Vertices needed before the chain can be closed back to its start.
const VERTICES_TO_CLOSE: usize = 3;

/// Unlike LINE, nothing is committed while the command runs: the polyline in
/// progress is transient, and one entity is created when the command ends.
/// Escape therefore discards it entirely.
#[derive(Debug, Default)]
pub(crate) struct PlineCommand {
    vertices: Vec<Point2>,
}

impl PlineCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn finish(&mut self, closed: bool, ctx: &mut CommandContext<'_>) -> Step {
        if self.vertices.len() >= 2 {
            let mut transaction = ctx.drawing.transaction();
            transaction.insert(PolylineEntity::new(
                std::mem::take(&mut self.vertices),
                closed,
            ));
            transaction.commit();
        }
        Step::Finished
    }
}

impl Command for PlineCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(if self.vertices.is_empty() {
            "Specify start point"
        } else {
            "Specify next point"
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.vertices.len() {
            0 => ExpectedInput::point(),
            count if count >= VERTICES_TO_CLOSE => ExpectedInput::point()
                .with_direct_distance()
                .with_keywords(&[CLOSE, UNDO]),
            _ => ExpectedInput::point()
                .with_direct_distance()
                .with_keywords(&[UNDO]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Point(point) => {
                let repeats_last = self
                    .vertices
                    .last()
                    .is_some_and(|last| LinearTolerance::DEFAULT.coincident(*last, point));
                if repeats_last {
                    ctx.message("Zero-length segment ignored.");
                } else {
                    self.vertices.push(point);
                }
            }
            CommandInput::Keyword(keyword) if keyword == UNDO => {
                if self.vertices.len() > 1 {
                    self.vertices.pop();
                } else {
                    ctx.message("All segments already undone.");
                }
            }
            CommandInput::Keyword(keyword) if keyword == CLOSE => return self.finish(true, ctx),
            CommandInput::Accept => return self.finish(false, ctx),
            CommandInput::Keyword(_) | CommandInput::Distance(_) | CommandInput::Angle(_) => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if !self.vertices.is_empty() {
            let mut vertices = self.vertices.clone();
            vertices.push(cursor);
            out.push(OverlayItem::PreviewEntity(Entity::Polyline(
                PolylineEntity::new(vertices, false),
            )));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.vertices.last().copied()
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use cad_document::{Entity, PolylineEntity};

    use crate::OverlayItem;
    use crate::testing::Harness;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn polylines(h: &Harness) -> Vec<PolylineEntity> {
        h.entities()
            .into_iter()
            .filter_map(|entity| match entity {
                Entity::Polyline(polyline) => Some(polyline),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_open_polyline_is_one_entity() {
        let mut h = Harness::new();
        h.enter("PLINE");
        assert_eq!(h.prompt(), "Specify start point:");
        h.enter_all(&["0,0", "10,0"]);
        assert_eq!(h.prompt(), "Specify next point or [Undo]:");
        h.enter("@0,5");
        assert_eq!(h.prompt(), "Specify next point or [Close/Undo]:");
        assert!(
            h.entities().is_empty(),
            "nothing is committed until the command ends"
        );

        h.enter("");
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(
            polylines(&h),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)],
                false
            )]
        );
        assert_eq!(h.entities().len(), 1);
    }

    #[test]
    fn close_makes_a_closed_polyline_without_repeating_the_first_vertex() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "10,10", "C"]);
        assert_eq!(
            polylines(&h),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)],
                true
            )]
        );
    }

    #[test]
    fn close_needs_three_vertices() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "C"]);
        assert_eq!(
            h.log().last().unwrap(),
            "A point or an option keyword is required."
        );
        assert!(h.entities().is_empty());
    }

    #[test]
    fn undo_steps_back_one_vertex_but_keeps_the_start() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "10,10", "U", "U", "U"]);
        assert_eq!(h.log().last().unwrap(), "All segments already undone.");
        h.enter_all(&["0,7", ""]);
        assert_eq!(
            polylines(&h),
            vec![PolylineEntity::new(vec![p(0.0, 0.0), p(0.0, 7.0)], false)]
        );
    }

    #[test]
    fn finishing_with_fewer_than_two_vertices_creates_nothing() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "5,5", ""]);
        assert!(h.entities().is_empty());
        assert!(!h.session.ui_state().can_undo);
    }

    #[test]
    fn escape_discards_the_whole_polyline() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "10,10"]);
        h.escape();
        assert!(h.entities().is_empty());
        assert!(!h.session.ui_state().can_undo);
    }

    #[test]
    fn repeated_points_are_ignored() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "0,0", "@0,0", "5,0", ""]);
        assert_eq!(polylines(&h)[0].vertices.len(), 2);
    }

    #[test]
    fn the_preview_is_the_polyline_so_far_plus_the_cursor() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0"]);
        h.move_to(410.0, 290.0); // world (10, 10)
        let expected = PolylineEntity::new(vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], false);
        assert!(
            h.session
                .overlay()
                .contains(&OverlayItem::PreviewEntity(Entity::Polyline(expected)))
        );
    }

    #[test]
    fn a_polyline_is_one_undo_step_and_is_picked_on_any_segment() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "-50,0", "50,0", "50,60", ""]);
        h.click_world(p(50.0, 30.0));
        assert_eq!(h.session.selection().len(), 1);
        h.undo();
        assert!(h.entities().is_empty());
    }

    #[test]
    fn direct_distance_works_between_vertices() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0"]);
        h.move_to(700.0, 300.0); // due east
        h.enter_all(&["25", ""]);
        assert_eq!(polylines(&h)[0].vertices, vec![p(0.0, 0.0), p(25.0, 0.0)]);
    }
}
