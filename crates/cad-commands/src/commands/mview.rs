//! MVIEW (#192): a viewport on a layout's paper — a window onto model
//! space — by two corners, or fitted to the sheet's printable area. The
//! model's extents are what it shows, at the scale that fits them.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2};
use cad_document::{Entity, LayoutId, PolylineEntity, ViewportEntity};
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const FIT: Keyword = Keyword::new("Fit");

#[derive(Debug, Default)]
pub(crate) struct MviewCommand {
    first: Option<Point2>,
}

impl MviewCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The window with `a` and `b` as opposite corners, or `None` when it
    /// has no area.
    fn window(a: Point2, b: Point2) -> Option<Aabb> {
        let size = b - a;
        let tolerance = LinearTolerance::DEFAULT.around(&[a, b]);
        if tolerance.is_zero(size.x) || tolerance.is_zero(size.y) {
            return None;
        }
        Some(Aabb::from_points(a, b))
    }

    /// A viewport over `frame` showing the model's extents at the scale
    /// that fits them, committed.
    fn make(ctx: &mut CommandContext<'_>, frame: &Aabb) -> Step {
        let extents = ctx.drawing.extents_in(LayoutId::MODEL);
        let viewport = ViewportEntity::fitting(frame, extents.as_ref());
        let mut transaction = ctx.transaction();
        match transaction.insert(viewport) {
            Ok(_) => {
                transaction.commit();
                let ratio = crate::summary::number(1.0 / viewport.scale);
                let ratio = ratio.trim_end_matches('0').trim_end_matches('.');
                ctx.message(format!("Viewport at 1:{ratio} of the model."));
            }
            Err(error) => ctx.message(format!("Cannot make the viewport: {error}.")),
        }
        Step::Finished
    }
}

impl Command for MviewCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.space == LayoutId::MODEL {
            ctx.message("MVIEW makes a viewport on a layout's paper: go to a layout first.");
            return Step::Finished;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(match self.first {
            None => "Specify corner of viewport",
            Some(_) => "Specify opposite corner",
        })
    }

    fn expected(&self) -> ExpectedInput {
        match self.first {
            None => ExpectedInput::point().with_keywords(&[FIT]),
            Some(_) => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (self.first, input) {
            (_, CommandInput::Accept) => return Step::Finished,
            (None, CommandInput::Keyword(_)) => {
                let Some(layout) = ctx.drawing.layouts().get(ctx.space) else {
                    return Step::Finished;
                };
                let frame = layout.printable_area();
                return Self::make(ctx, &frame);
            }
            (None, CommandInput::Point(corner)) => self.first = Some(corner),
            (Some(first), CommandInput::Point(other)) => match Self::window(first, other) {
                Some(frame) => return Self::make(ctx, &frame),
                None => ctx.message("The viewport has no area."),
            },
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(frame) = self.first.and_then(|first| Self::window(first, cursor)) {
            out.push(OverlayItem::PreviewEntity(Entity::Polyline(
                PolylineEntity::new(frame.corners().to_vec(), true),
            )));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.first
    }

    /// The corners are places on the paper, not points of the drawing.
    fn takes_object_snap(&self) -> bool {
        false
    }
}
