//! SPLINE (#182): a cubic through the points typed or picked, in order,
//! finished by Enter after at least two; `Undo` takes the last point back.

use cad_core::Point2;
use cad_document::SplineEntity;
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const UNDO: Keyword = Keyword::new("Undo");

#[derive(Debug)]
pub(crate) struct SplineCommand {
    points: Vec<Point2>,
}

impl SplineCommand {
    pub(crate) fn new() -> Self {
        Self { points: Vec::new() }
    }

    /// The spline the points so far and `next` would make.
    fn candidate(&self, next: Point2) -> Option<SplineEntity> {
        let mut points = self.points.clone();
        if points.last() != Some(&next) {
            points.push(next);
        }
        SplineEntity::through(&points)
    }
}

impl Command for SplineCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(match self.points.len() {
            0 => "Specify first point",
            _ => "Specify next point",
        })
    }

    fn expected(&self) -> ExpectedInput {
        if self.points.is_empty() {
            ExpectedInput::point()
        } else {
            ExpectedInput::point().with_keywords(&[UNDO])
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.points.last().copied()
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(last) = self.points.last() {
            out.push(OverlayItem::PreviewSegment {
                start: *last,
                end: cursor,
            });
        }
        if let Some(spline) = self.candidate(cursor) {
            out.push(OverlayItem::PreviewEntity(spline.into()));
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Point(point) => {
                if self.points.last() == Some(&point) {
                    ctx.message("The point is the same as the last.");
                } else {
                    self.points.push(point);
                }
                Step::Continue
            }
            CommandInput::Keyword(keyword) if keyword == UNDO => {
                self.points.pop();
                Step::Continue
            }
            CommandInput::Accept => {
                if self.points.len() < 2 {
                    // Nothing to make: Enter leaves quietly.
                    return Step::Finished;
                }
                let Some(spline) = SplineEntity::through(&self.points) else {
                    ctx.message("These points do not make a spline.");
                    return Step::Continue;
                };
                let mut transaction = ctx.drawing.transaction();
                transaction
                    .insert(spline)
                    .expect("a spline has no text style to miss");
                transaction.commit();
                Step::Finished
            }
            _ => Step::Continue,
        }
    }
}
