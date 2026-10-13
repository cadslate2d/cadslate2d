//! ZOOM and PAN (#180): the view moved by name. The view is session state,
//! not drawing state: neither command commits anything, `U` does not undo
//! them, and ZOOM Previous keeps a stack of its own.

use cad_core::{Aabb, Point2, View};
use cad_document::{Layout, LayoutId};
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const ALL: Keyword = Keyword::new("All");
const CENTER: Keyword = Keyword::new("Center");
const EXTENTS: Keyword = Keyword::new("Extents");
const PREVIOUS: Keyword = Keyword::new("Previous");
const SCALE: Keyword = Keyword::new("Scale");
const WINDOW: Keyword = Keyword::new("Window");

/// How many views ZOOM Previous remembers.
const PREVIOUS_VIEWS: usize = 10;

/// Moves the view to `next`, remembering where it was.
fn move_view(ctx: &mut CommandContext<'_>, next: View) {
    if next == *ctx.view {
        return;
    }
    ctx.previous_views.push(ctx.view.clone());
    if ctx.previous_views.len() > PREVIOUS_VIEWS {
        ctx.previous_views.remove(0);
    }
    *ctx.view = next;
}

/// The view fitted to the drawing's extents; where there is nothing, the
/// view as it is.
fn extents(ctx: &mut CommandContext<'_>) -> Step {
    // On a layout the extents are the sheet (#193).
    let extents = if ctx.space == LayoutId::MODEL {
        ctx.drawing.extents_in(ctx.space)
    } else {
        ctx.drawing.layouts().get(ctx.space).map(Layout::sheet)
    };
    match extents {
        Some(extents) => {
            let mut next = ctx.view.clone();
            next.fit(&extents);
            move_view(ctx, next);
        }
        None => ctx.message("Nothing to zoom to: the drawing is empty."),
    }
    Step::Finished
}

#[derive(Debug)]
enum ZoomState {
    /// The first prompt: a window's corner, or an option.
    Start,
    /// The window's first corner is known.
    OppositeCorner(Point2),
    /// `Scale`: waiting for the factor.
    Factor,
    /// `Center`: waiting for the point.
    CenterPoint,
    /// `Center`: the point is known, waiting for the height.
    CenterHeight(Point2),
}

#[derive(Debug)]
pub(crate) struct ZoomCommand {
    state: ZoomState,
}

impl ZoomCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: ZoomState::Start,
        }
    }
}

impl Command for ZoomCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            ZoomState::Start => Prompt::new("Specify corner of window").with_default("Extents"),
            ZoomState::OppositeCorner(_) => Prompt::new("Specify opposite corner"),
            ZoomState::Factor => Prompt::new("Enter a scale factor (2x, 0.5x)"),
            ZoomState::CenterPoint => Prompt::new("Specify center point"),
            ZoomState::CenterHeight(_) => Prompt::new("Enter view height"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            ZoomState::Start => ExpectedInput::point()
                .with_keywords(&[ALL, CENTER, EXTENTS, PREVIOUS, SCALE, WINDOW]),
            ZoomState::OppositeCorner(_) | ZoomState::CenterPoint => ExpectedInput::point(),
            ZoomState::Factor => ExpectedInput::factor(),
            ZoomState::CenterHeight(_) => ExpectedInput::distance(),
        }
    }

    /// A window's corners are places on the screen, not points of the
    /// drawing; the centre is a point of the drawing.
    fn takes_object_snap(&self) -> bool {
        matches!(self.state, ZoomState::CenterPoint)
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            ZoomState::OppositeCorner(corner) => Some(*corner),
            _ => None,
        }
    }

    fn tracking_base(&self) -> Option<Point2> {
        None
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (ZoomState::Start, CommandInput::Accept) => extents(ctx),
            (ZoomState::Start, CommandInput::Keyword(keyword)) => match keyword {
                k if k == EXTENTS || k == ALL => extents(ctx),
                k if k == PREVIOUS => {
                    match ctx.previous_views.pop() {
                        Some(previous) => *ctx.view = previous,
                        None => ctx.message("No previous view."),
                    }
                    Step::Finished
                }
                k if k == SCALE => {
                    self.state = ZoomState::Factor;
                    Step::Continue
                }
                k if k == CENTER => {
                    self.state = ZoomState::CenterPoint;
                    Step::Continue
                }
                _ => Step::Continue, // Window: the corner comes next
            },
            (ZoomState::Start, CommandInput::Point(corner)) => {
                self.state = ZoomState::OppositeCorner(corner);
                Step::Continue
            }
            (ZoomState::OppositeCorner(first), CommandInput::Point(second)) => {
                let window = Aabb::from_points(*first, second);
                let size = window.max() - window.min();
                if size.x <= 0.0 && size.y <= 0.0 {
                    ctx.message("The window has no size.");
                    return Step::Continue;
                }
                let mut next = ctx.view.clone();
                next.fit(&window);
                move_view(ctx, next);
                Step::Finished
            }
            (ZoomState::Factor, CommandInput::Distance(factor)) => {
                if !(factor.is_finite() && factor > 0.0) {
                    ctx.message("The factor must be a positive number.");
                    return Step::Continue;
                }
                let mut next = ctx.view.clone();
                let middle = cad_core::ScreenPoint::new(next.width() / 2.0, next.height() / 2.0);
                next.zoom_about(middle, factor);
                move_view(ctx, next);
                Step::Finished
            }
            (ZoomState::CenterPoint, CommandInput::Point(center)) => {
                self.state = ZoomState::CenterHeight(center);
                Step::Continue
            }
            (ZoomState::CenterHeight(center), CommandInput::Accept) => {
                let mut next = ctx.view.clone();
                next.look_at(*center);
                move_view(ctx, next);
                Step::Finished
            }
            (ZoomState::CenterHeight(center), CommandInput::Distance(height)) => {
                if !(height.is_finite() && height > 0.0) {
                    ctx.message("The height must be a positive length.");
                    return Step::Continue;
                }
                let mut next = ctx.view.clone();
                next.look_at(*center);
                next.set_height(height);
                move_view(ctx, next);
                Step::Finished
            }
            (_, CommandInput::Accept) => Step::Finished,
            _ => Step::Continue,
        }
    }
}

#[derive(Debug)]
pub(crate) struct PanCommand {
    /// The base point, once given.
    from: Option<Point2>,
}

impl PanCommand {
    pub(crate) fn new() -> Self {
        Self { from: None }
    }
}

impl Command for PanCommand {
    fn prompt(&self) -> Prompt {
        match self.from {
            None => Prompt::new("Specify base point or displacement"),
            Some(_) => {
                Prompt::new("Specify second point").with_default("use first point as displacement")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn base_point(&self) -> Option<Point2> {
        self.from
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (self.from, input) {
            (None, CommandInput::Point(from)) => {
                self.from = Some(from);
                Step::Continue
            }
            (Some(from), CommandInput::Point(to)) => {
                let mut next = ctx.view.clone();
                next.pan_by_world(to - from);
                move_view(ctx, next);
                Step::Finished
            }
            // Enter: the first point is a displacement from the origin.
            (Some(from), CommandInput::Accept) => {
                let mut next = ctx.view.clone();
                next.pan_by_world(from - Point2::ORIGIN);
                move_view(ctx, next);
                Step::Finished
            }
            (_, CommandInput::Accept) => Step::Finished,
            _ => Step::Continue,
        }
    }
}
