//! BOUNDARY (#229): the loop HATCH would fill round a point, drawn as a
//! closed polyline.
//!
//! The loop is HATCH's — the smallest closed loop of what is shown round
//! the point picked (`hatch::region_at`) — and, with `Islands` on, the
//! loops inside it too, each a closed polyline of its own, its arcs as
//! bulges, on the current layer. Points until Enter; the invocation is one
//! undo step.

use cad_core::Point2;
use cad_document::PolylineEntity;
use cad_input::{ExpectedInput, Keyword};

use super::hatch::{Candidate, candidates, region_at};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const ISLANDS: Keyword = Keyword::new("Islands");

#[derive(Debug)]
pub(crate) struct BoundaryCommand {
    candidates: Vec<Candidate>,
    /// Whether the loops inside the boundary are drawn too.
    islands: bool,
}

impl BoundaryCommand {
    pub(crate) fn new() -> Self {
        Self {
            candidates: Vec::new(),
            islands: true,
        }
    }
}

impl Command for BoundaryCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.candidates = candidates(ctx);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Pick internal point")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point().with_keywords(&[ISLANDS])
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Point(point) => {
                let Some(mut loops) = region_at(&self.candidates, point) else {
                    ctx.message("No closed boundary surrounds that point.");
                    return Step::Continue;
                };
                if !self.islands {
                    loops.truncate(1);
                }
                let count = loops.len();
                let mut transaction = ctx.transaction();
                for path in loops {
                    transaction
                        .insert(PolylineEntity::from(path))
                        .expect("a closed loop of finite points");
                }
                transaction.commit();
                ctx.message(if count == 1 {
                    "BOUNDARY created 1 polyline.".to_owned()
                } else {
                    format!("BOUNDARY created {count} polylines.")
                });
                // What it drew bounds what is picked next, as HATCH sees it.
                self.candidates = candidates(ctx);
            }
            CommandInput::Keyword(_) => {
                self.islands = !self.islands;
                ctx.message(if self.islands {
                    "Islands are detected."
                } else {
                    "Islands are not detected."
                });
            }
            CommandInput::Accept => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// A point inside a region is a place, not a point of the drawing.
    fn takes_object_snap(&self) -> bool {
        false
    }

    fn base_point(&self) -> Option<Point2> {
        None
    }
}
