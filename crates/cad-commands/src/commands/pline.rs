//! PLINE: one polyline entity through a chain of points, joined by straight
//! segments or — with the `Arc` option — by arcs that continue smoothly from
//! the segment before.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};
use cad_document::{Entity, PolylineEntity};
use cad_geometry::{bulge_end_tangent, bulge_from_tangent};
use cad_input::{ExpectedInput, Keyword};

use super::keywords::{CLOSE, UNDO};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const ARC: Keyword = Keyword::new("Arc");
const LINE: Keyword = Keyword::new("Line");

/// Vertices needed before the chain can be closed back to its start.
const VERTICES_TO_CLOSE: usize = 3;
/// The direction an arc leaves the start point in when no segment comes
/// before it: along +X.
const DEFAULT_TANGENT: Vec2 = Vec2::new(1.0, 0.0);

/// What the next point adds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Line,
    /// An arc tangent to the end of the previous segment.
    Arc,
}

/// Unlike LINE, nothing is committed while the command runs: the polyline in
/// progress is transient, and one entity is created when the command ends.
/// Escape therefore discards it entirely.
#[derive(Debug, Default)]
pub(crate) struct PlineCommand {
    vertices: Vec<Point2>,
    /// The bulge of each segment so far: one fewer than the vertices.
    bulges: Vec<f64>,
    mode: Mode,
}

impl PlineCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The direction of travel on arriving at the last vertex.
    fn tangent(&self) -> Vec2 {
        match self.vertices.as_slice() {
            [.., before, last] => {
                let bulge = self.bulges.last().copied().unwrap_or(0.0);
                bulge_end_tangent(*before, *last, bulge)
            }
            _ => DEFAULT_TANGENT,
        }
    }

    /// The bulge of the segment that would run from the last vertex to
    /// `to` in the current mode. `None` when no such segment exists: an arc
    /// cannot come straight back along its own tangent.
    fn bulge_to(&self, to: Point2) -> Option<f64> {
        match (self.mode, self.vertices.last()) {
            (Mode::Arc, Some(last)) => bulge_from_tangent(*last, self.tangent(), to),
            _ => Some(0.0),
        }
    }

    /// The polyline so far, with one more segment to `to` when there is one.
    fn with_segment_to(&self, to: Point2) -> Option<PolylineEntity> {
        let bulge = self.bulge_to(to)?;
        let vertices = self.vertices.iter().copied().chain([to]).collect();
        let bulges = self.bulges.iter().copied().chain([bulge]).collect();
        Some(PolylineEntity::with_bulges(vertices, bulges, false))
    }

    fn finish(&mut self, closed: bool, ctx: &mut CommandContext<'_>) -> Step {
        if self.vertices.len() < 2 {
            return Step::Finished;
        }
        if closed {
            // The closing segment follows the current mode like any other.
            let closing = self.bulge_to(self.vertices[0]).unwrap_or_else(|| {
                ctx.message("An arc cannot close the polyline from here; closing with a line.");
                0.0
            });
            self.bulges.push(closing);
        }
        let bulges = std::mem::take(&mut self.bulges);
        let polyline =
            PolylineEntity::with_bulges(std::mem::take(&mut self.vertices), bulges, closed);
        let mut transaction = ctx.transaction();
        transaction
            .insert(polyline)
            .expect("a polyline has no text style to miss");
        transaction.commit();
        Step::Finished
    }
}

impl Command for PlineCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match (self.vertices.is_empty(), self.mode) {
            (true, _) => "Specify start point",
            (false, Mode::Line) => "Specify next point",
            (false, Mode::Arc) => "Specify endpoint of arc",
        })
    }

    fn expected(&self) -> ExpectedInput {
        let switch = match self.mode {
            Mode::Line => ARC,
            Mode::Arc => LINE,
        };
        match self.vertices.len() {
            0 => ExpectedInput::point(),
            count if count >= VERTICES_TO_CLOSE => ExpectedInput::point()
                .with_direct_distance()
                .with_keywords(&[switch, CLOSE, UNDO]),
            _ => ExpectedInput::point()
                .with_direct_distance()
                .with_keywords(&[switch, UNDO]),
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
                } else if self.vertices.is_empty() {
                    self.vertices.push(point);
                } else {
                    match self.bulge_to(point) {
                        Some(bulge) => {
                            self.vertices.push(point);
                            self.bulges.push(bulge);
                        }
                        None => ctx.message(
                            "An arc cannot come straight back the way it left; pick another point.",
                        ),
                    }
                }
            }
            CommandInput::Keyword(keyword) if keyword == ARC => self.mode = Mode::Arc,
            CommandInput::Keyword(keyword) if keyword == LINE => self.mode = Mode::Line,
            CommandInput::Keyword(keyword) if keyword == UNDO => {
                if self.vertices.len() > 1 {
                    self.vertices.pop();
                    self.bulges.pop();
                } else {
                    ctx.message("All segments already undone.");
                }
            }
            CommandInput::Keyword(keyword) if keyword == CLOSE => return self.finish(true, ctx),
            CommandInput::Accept => return self.finish(false, ctx),
            CommandInput::Keyword(_)
            | CommandInput::Distance(_)
            | CommandInput::Angle(_)
            | CommandInput::Text(_)
            | CommandInput::Pick { .. }
            | CommandInput::File(_) => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if self.vertices.is_empty() {
            return;
        }
        if let Some(polyline) = self.with_segment_to(cursor) {
            out.push(OverlayItem::PreviewEntity(Entity::Polyline(polyline)));
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
        assert_eq!(h.prompt(), "Specify next point or [Arc/Undo]:");
        h.enter("@0,5");
        assert_eq!(h.prompt(), "Specify next point or [Arc/Close/Undo]:");
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

    // ── arc segments (#116) ──

    use cad_geometry::Primitive;

    const QUARTER: f64 = std::f64::consts::SQRT_2 - 1.0; // tan(90° / 4)

    fn close_to(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    /// Line–arc–line: the arc continues smoothly from the first line.
    #[test]
    fn arc_and_line_switch_between_arc_and_straight_segments() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "A"]);
        assert_eq!(h.prompt(), "Specify endpoint of arc or [Line/Undo]:");
        h.enter("20,10"); // leaving eastwards, turning up: a quarter circle
        assert_eq!(h.prompt(), "Specify endpoint of arc or [Line/Close/Undo]:");
        h.enter("L");
        assert_eq!(h.prompt(), "Specify next point or [Arc/Close/Undo]:");
        h.enter_all(&["20,30", ""]);

        let polyline = &polylines(&h)[0];
        assert_eq!(
            polyline.vertices,
            vec![p(0.0, 0.0), p(10.0, 0.0), p(20.0, 10.0), p(20.0, 30.0)]
        );
        let bulges: Vec<f64> = polyline.bulges().collect();
        assert!(close_to(bulges[1], QUARTER), "{bulges:?}");
        assert_eq!((bulges[0], bulges[2], bulges[3]), (0.0, 0.0, 0.0));
        // The arc is a quarter circle about (10, 10): tangent to both lines.
        let Some(Primitive::Arc(arc)) = polyline.primitives().nth(1) else {
            panic!("the second segment is an arc");
        };
        assert!(arc.center.distance_to(p(10.0, 10.0)) < 1e-9 && close_to(arc.radius, 10.0));
    }

    #[test]
    fn consecutive_arcs_stay_tangent_to_each_other() {
        let mut h = Harness::new();
        // An S: a quarter turn left, then a quarter turn right.
        h.enter_all(&["PL", "0,0", "10,0", "A", "20,10", "30,20", ""]);
        let bulges: Vec<f64> = polylines(&h)[0].bulges().collect();
        // Arriving at (20,10) heading north, then on to (30,20): clockwise.
        assert!(
            close_to(bulges[1], QUARTER) && close_to(bulges[2], -QUARTER),
            "{bulges:?}"
        );
    }

    #[test]
    fn a_first_arc_leaves_the_start_point_eastwards() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "A", "0,10", ""]);
        let polyline = &polylines(&h)[0];
        // Straight to the left of the direction of travel: a semicircle.
        assert!(close_to(polyline.bulge(0), 1.0));
        assert!(
            Entity::Polyline(polyline.clone())
                .distance_to(p(5.0, 5.0), cad_document::Tables::none())
                < 1e-9
        );
    }

    /// Closing with an arc.
    #[test]
    fn close_in_arc_mode_closes_with_a_tangent_arc() {
        let mut h = Harness::new();
        // Up the right side, then an arc over the top and one back to the start.
        h.enter_all(&["PL", "0,0", "10,0", "10,10", "A", "0,10", "C"]);
        let polyline = &polylines(&h)[0];
        assert!(polyline.closed);
        assert_eq!(polyline.vertices.len(), 4);
        let bulges: Vec<f64> = polyline.bulges().collect();
        // Heading north at (10,10), across to (0,10): a semicircle.
        assert!(close_to(bulges[2], 1.0), "{bulges:?}");
        // Arriving at (0,10) heading south, the start is straight ahead.
        assert!(bulges[3].abs() < 1e-12, "{bulges:?}");

        // A closing segment that does need an arc.
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "10,10", "A", "C"]);
        let polyline = &polylines(&h)[0];
        // From (10,10) heading north, back to the origin: more than a semicircle, turning left.
        assert!(
            polyline.closed && polyline.bulge(2) > 1.0,
            "{:?}",
            polyline.bulge(2)
        );
    }

    /// Undo within the command, across both kinds of segment.
    #[test]
    fn undo_removes_arc_and_straight_segments_alike() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "A", "20,10", "L", "20,30", "U", "U"]);
        // Back to the first segment; the mode stays where it was left.
        assert_eq!(h.prompt(), "Specify next point or [Arc/Undo]:");
        h.enter_all(&["10,5", ""]);
        assert_eq!(
            polylines(&h),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)],
                false
            )]
        );

        // Undoing an arc leaves no stray bulge behind.
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "A", "20,10", "U", "L", "30,0", ""]);
        assert_eq!(
            polylines(&h),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(10.0, 0.0), p(30.0, 0.0)],
                false
            )]
        );
    }

    #[test]
    fn an_arc_cannot_come_straight_back() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "A", "5,0"]);
        assert_eq!(
            h.log().last().unwrap(),
            "An arc cannot come straight back the way it left; pick another point."
        );
        assert_eq!(h.prompt(), "Specify endpoint of arc or [Line/Undo]:");
        h.enter_all(&["15,0", ""]); // straight ahead is simply a straight segment
        assert_eq!(
            polylines(&h),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(10.0, 0.0), p(15.0, 0.0)],
                false
            )]
        );
    }

    #[test]
    fn the_arc_is_previewed_as_it_would_be_drawn() {
        let mut h = Harness::new();
        h.enter_all(&["PL", "0,0", "10,0", "A"]);
        h.move_to(420.0, 290.0); // world (20, 10)
        let expected = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(20.0, 10.0)],
            vec![0.0, QUARTER],
            false,
        );
        let previewed = h.session.overlay().into_iter().find_map(|item| match item {
            OverlayItem::PreviewEntity(Entity::Polyline(polyline)) => Some(polyline),
            _ => None,
        });
        let previewed = previewed.expect("a polyline preview");
        assert_eq!(previewed.vertices, expected.vertices);
        assert!(close_to(previewed.bulge(1), QUARTER));
        assert!(h.entities().is_empty());
    }
}
