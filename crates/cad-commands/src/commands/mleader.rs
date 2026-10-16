//! MLEADER (#240, ADR-0033): a multileader from its arrowhead to its
//! landing, then its note — words, or Enter for none — laid out by the
//! current multileader style. One entity, one undo.

use cad_core::Point2;
use cad_document::{MLeaderContent, MLeaderEntity, MLeaderStyleId};
use cad_input::ExpectedInput;

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

#[derive(Debug)]
enum State {
    Arrowhead,
    Landing(Point2),
    Note(Point2, Point2),
}

#[derive(Debug)]
pub(crate) struct MLeaderCommand {
    state: State,
    style: MLeaderStyleId,
}

impl MLeaderCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Arrowhead,
            style: MLeaderStyleId::FIRST,
        }
    }

    fn make(&self, tip: Point2, landing: Point2, content: MLeaderContent) -> MLeaderEntity {
        MLeaderEntity::new(tip, landing, content).in_style(self.style)
    }
}

impl Command for MLeaderCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.settings().current_mleader_style;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Arrowhead => Prompt::new("Specify leader arrowhead location"),
            State::Landing(_) => Prompt::new("Specify leader landing location"),
            State::Note(..) => Prompt::new("Enter the note").with_default("none"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Arrowhead => ExpectedInput::point(),
            State::Landing(_) => ExpectedInput::point().with_direct_distance(),
            State::Note(..) => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let (tip, landing, content) = match (&self.state, input) {
            (State::Arrowhead, CommandInput::Point(tip)) => {
                self.state = State::Landing(tip);
                return Step::Continue;
            }
            (State::Landing(tip), CommandInput::Point(landing)) => {
                self.state = State::Note(*tip, landing);
                return Step::Continue;
            }
            (State::Note(tip, landing), CommandInput::Text(words)) if !words.trim().is_empty() => {
                (*tip, *landing, MLeaderContent::Text(words))
            }
            (State::Note(tip, landing), CommandInput::Text(_) | CommandInput::Accept) => {
                (*tip, *landing, MLeaderContent::None)
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => return Step::Continue,
        };
        let mut transaction = ctx.transaction();
        match transaction.insert(self.make(tip, landing, content)) {
            Ok(_) => {
                transaction.commit();
            }
            Err(error) => {
                drop(transaction);
                ctx.message(error.to_string());
            }
        }
        Step::Finished
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Landing(tip) = self.state {
            out.push(OverlayItem::PreviewEntity(
                self.make(tip, cursor, MLeaderContent::None).into(),
            ));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::Landing(tip) => Some(tip),
            _ => None,
        }
    }
}
