//! RECTANG: a closed polyline from two opposite corners.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{Entity, PolylineEntity};
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

#[derive(Debug, Default)]
pub(crate) struct RectangCommand {
    first: Option<Point2>,
}

impl RectangCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

/// The axis-aligned rectangle with `a` and `b` as opposite corners, or `None`
/// when it has no area.
fn rectangle(a: Point2, b: Point2) -> Option<PolylineEntity> {
    let size = b - a;
    let tolerance = LinearTolerance::DEFAULT;
    if tolerance.is_zero(size.x) || tolerance.is_zero(size.y) {
        return None;
    }
    let corners = vec![a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)];
    Some(PolylineEntity::new(corners, true))
}

impl Command for RectangCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match self.first {
            None => "Specify first corner point",
            Some(_) => "Specify other corner point",
        })
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (self.first, input) {
            (_, CommandInput::Accept) => return Step::Finished,
            (None, CommandInput::Point(corner)) => self.first = Some(corner),
            (Some(first), CommandInput::Point(other)) => match rectangle(first, other) {
                Some(polyline) => {
                    let mut transaction = ctx.drawing.transaction();
                    transaction.insert(polyline);
                    transaction.commit();
                    return Step::Finished;
                }
                None => ctx.message("The rectangle has no area."),
            },
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(polyline) = self.first.and_then(|first| rectangle(first, cursor)) {
            out.push(OverlayItem::PreviewEntity(Entity::Polyline(polyline)));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.first
    }

    /// The opposite corner is not at the end of a rubber-band line: ORTHO
    /// would flatten the rectangle.
    fn tracking_base(&self) -> Option<Point2> {
        None
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

    fn only_polyline(h: &Harness) -> PolylineEntity {
        match h.entities().as_slice() {
            [Entity::Polyline(polyline)] => polyline.clone(),
            other => panic!("expected exactly one polyline, found {other:?}"),
        }
    }

    /// The headless test named in the issue: `REC`, `0,0`, `@100,50`.
    #[test]
    fn two_corners_make_a_closed_polyline_with_four_vertices() {
        let mut h = Harness::new();
        h.enter("REC");
        assert_eq!(h.prompt(), "Specify first corner point:");
        h.enter("0,0");
        assert_eq!(h.prompt(), "Specify other corner point:");
        h.enter("@100,50");
        assert_eq!(h.prompt(), "Command:");

        let rectangle = only_polyline(&h);
        assert!(rectangle.closed);
        assert_eq!(
            rectangle.vertices,
            vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0), p(0.0, 50.0)]
        );
    }

    #[test]
    fn corners_may_be_given_in_any_order_and_by_pointer() {
        let mut h = Harness::new();
        h.enter("RECTANG");
        h.click_world(p(30.0, 40.0));
        h.click_world(p(-10.0, -20.0));
        let rectangle = only_polyline(&h);
        assert_eq!(
            rectangle.vertices,
            vec![
                p(30.0, 40.0),
                p(-10.0, 40.0),
                p(-10.0, -20.0),
                p(30.0, -20.0)
            ]
        );
    }

    #[test]
    fn a_rectangle_without_area_is_refused() {
        let mut h = Harness::new();
        h.enter_all(&["REC", "0,0", "@100,0"]);
        assert_eq!(h.log().last().unwrap(), "The rectangle has no area.");
        h.enter("0,0");
        assert_eq!(h.log().last().unwrap(), "The rectangle has no area.");
        assert_eq!(h.prompt(), "Specify other corner point:");
        assert!(h.entities().is_empty());

        h.enter("5,5");
        assert_eq!(h.entities().len(), 1);
    }

    #[test]
    fn the_preview_follows_the_cursor() {
        let mut h = Harness::new();
        h.enter_all(&["REC", "0,0"]);
        h.move_to(440.0, 280.0); // world (40, 20)
        let expected = PolylineEntity::new(
            vec![p(0.0, 0.0), p(40.0, 0.0), p(40.0, 20.0), p(0.0, 20.0)],
            true,
        );
        assert!(
            h.session
                .overlay()
                .contains(&OverlayItem::PreviewEntity(Entity::Polyline(expected)))
        );
        assert!(h.entities().is_empty());
    }

    #[test]
    fn a_rectangle_is_one_entity_and_one_undo_step() {
        let mut h = Harness::new();
        h.enter_all(&["REC", "0,0", "10,10"]);
        h.undo();
        assert!(h.entities().is_empty());
        h.redo();
        assert_eq!(h.entities().len(), 1);
    }
}
