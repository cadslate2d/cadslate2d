//! SKETCH (#326): freehand drawing. A click puts the pen down; as the
//! pointer moves a vertex is recorded each time it has gone the record
//! increment from the last; a click lifts the pen, keeping where it was;
//! another click puts it down again for a new stroke. Enter keeps every
//! stroke of two vertices or more as an open polyline, in one transaction;
//! Escape keeps nothing. The path is the one `Session::aim` makes the
//! pointer into, so ORTHO and grid snap shape it as they shape any point.

use cad_core::Point2;
use cad_document::PolylineEntity;
use cad_input::{ExpectedInput, Keyword};

use super::ltscale::display;
use super::modify::counted;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const INCREMENT: Keyword = Keyword::new("Increment");

/// The record increment when none is given: this many pixels at the view
/// the command started in.
const DEFAULT_PIXELS: f64 = 4.0;

#[derive(Debug)]
enum State {
    Sketching,
    Increment,
}

#[derive(Debug)]
pub(crate) struct SketchCommand {
    state: State,
    increment: f64,
    /// The strokes kept so far; the last is being drawn while the pen is
    /// down.
    strokes: Vec<Vec<Point2>>,
    pen_down: bool,
}

impl SketchCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Sketching,
            increment: 1.0,
            strokes: Vec::new(),
            pen_down: false,
        }
    }

    fn commit(&self, ctx: &mut CommandContext<'_>) -> Step {
        let kept: Vec<&Vec<Point2>> = self.strokes.iter().filter(|s| s.len() >= 2).collect();
        if kept.is_empty() {
            ctx.message("Nothing sketched.");
            return Step::Finished;
        }
        let mut transaction = ctx.transaction();
        for stroke in &kept {
            transaction
                .insert(PolylineEntity::new((*stroke).clone(), false))
                .expect("a polyline has no style to miss");
        }
        transaction.commit();
        ctx.message(format!(
            "{} recorded.",
            counted(kept.len(), "sketched polyline")
        ));
        Step::Finished
    }
}

impl Command for SketchCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.increment = ctx.view.pixels_to_world(DEFAULT_PIXELS);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Sketching if self.pen_down => {
                Prompt::new("Sketching: click to lift the pen, Enter to keep the sketch")
            }
            State::Sketching => {
                Prompt::new("Click to put the pen down, or press Enter to keep the sketch")
            }
            State::Increment => {
                Prompt::new("Specify record increment").with_default(display(self.increment))
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Sketching => ExpectedInput::point().with_keywords(&[INCREMENT]),
            State::Increment => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Sketching, CommandInput::Point(at)) => {
                if self.pen_down {
                    // The pen lifts where it is, keeping that point.
                    if let Some(stroke) = self.strokes.last_mut()
                        && stroke.last().is_none_or(|last| last.distance_to(at) > 0.0)
                    {
                        stroke.push(at);
                    }
                    self.pen_down = false;
                } else {
                    self.strokes.push(vec![at]);
                    self.pen_down = true;
                }
            }
            (State::Sketching, CommandInput::Keyword(k)) if k == INCREMENT => {
                self.state = State::Increment;
            }
            (State::Sketching, CommandInput::Accept) => return self.commit(ctx),
            (State::Increment, CommandInput::Distance(value)) => {
                if value.is_finite() && value > 0.0 {
                    self.increment = value;
                } else {
                    ctx.message("The record increment must be greater than zero.");
                }
                self.state = State::Sketching;
            }
            (State::Increment, CommandInput::Accept) => self.state = State::Sketching,
            _ => {}
        }
        Step::Continue
    }

    fn pointer_moved(&mut self, at: Point2) {
        if !self.pen_down {
            return;
        }
        if let Some(stroke) = self.strokes.last_mut()
            && stroke
                .last()
                .is_some_and(|last| last.distance_to(at) >= self.increment)
        {
            stroke.push(at);
        }
    }

    /// While the pen is down, the last vertex: what ORTHO and POLAR aim from.
    fn base_point(&self) -> Option<Point2> {
        self.pen_down
            .then(|| self.strokes.last().and_then(|s| s.last().copied()))
            .flatten()
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        for (index, stroke) in self.strokes.iter().enumerate() {
            let mut points = stroke.clone();
            // The stroke being drawn reaches the pointer.
            if self.pen_down && index + 1 == self.strokes.len() {
                points.push(cursor);
            }
            if points.len() >= 2 {
                out.push(OverlayItem::PreviewEntity(
                    PolylineEntity::new(points, false).into(),
                ));
            }
        }
    }
}
