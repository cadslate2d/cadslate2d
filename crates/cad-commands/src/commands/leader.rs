//! LEADER (#205): points until Enter — the first is the arrowhead's tip
//! — then the note, or Enter for none; one entity, laid out by the
//! current dimension style.

use cad_core::Point2;
use cad_document::LeaderEntity;
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const UNDO: Keyword = Keyword::new("Undo");

#[derive(Debug)]
enum State {
    Points,
    Text,
}

#[derive(Debug)]
pub(crate) struct LeaderCommand {
    state: State,
    points: Vec<Point2>,
}

impl LeaderCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Points,
            points: Vec::new(),
        }
    }

    fn finish(&self, ctx: &mut CommandContext<'_>, text: Option<String>) -> Step {
        let leader = LeaderEntity::new(self.points.clone())
            .in_style(ctx.drawing.current_dim_style())
            .saying(text);
        let mut tx = ctx.transaction();
        tx.insert(leader)
            .expect("two finite points and the current style");
        tx.commit();
        Step::Finished
    }
}

impl Command for LeaderCommand {
    fn prompt(&self) -> Prompt {
        match (&self.state, self.points.len()) {
            (State::Points, 0) => Prompt::new("Specify leader start point"),
            (State::Points, _) => Prompt::new("Specify next point"),
            (State::Text, _) => Prompt::new("Enter leader text").with_default("none"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match (&self.state, self.points.len()) {
            (State::Points, 0 | 1) => ExpectedInput::point(),
            (State::Points, _) => ExpectedInput::point().with_keywords(&[UNDO]),
            (State::Text, _) => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Points, CommandInput::Point(point)) => {
                if self.points.last().is_some_and(|last| *last == point) {
                    ctx.message("The next point must differ from the last.");
                } else {
                    self.points.push(point);
                }
            }
            (State::Points, CommandInput::Keyword(_)) => {
                self.points.pop();
            }
            (State::Points, CommandInput::Accept) => {
                if self.points.len() < 2 {
                    return Step::Finished;
                }
                self.state = State::Text;
            }
            (State::Text, CommandInput::Text(text)) => return self.finish(ctx, Some(text)),
            (State::Text, CommandInput::Accept) => return self.finish(ctx, None),
            _ => {}
        }
        Step::Continue
    }

    /// The leader so far, its next segment following the pointer.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let (State::Points, Some(_)) = (&self.state, self.points.first()) {
            let mut points = self.points.clone();
            points.push(cursor);
            out.push(OverlayItem::PreviewEntity(LeaderEntity::new(points).into()));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::Points => self.points.last().copied(),
            State::Text => None,
        }
    }
}
