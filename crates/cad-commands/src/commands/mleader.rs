//! MLEADER (#240, #241; ADR-0033): a multileader from its arrowhead to its
//! landing, then its note, laid out by the current multileader style. One
//! entity, one undo.
//!
//! The note is words — asked for after the landing, Enter for none — or a
//! block, placed at the dogleg's end. `Content`, at the first prompt,
//! chooses: `Block` and its name, `Mtext`, or `None`. A style that names a
//! block starts with that block.

use cad_core::Point2;
use cad_document::{BlockId, MLeaderContent, MLeaderEntity, MLeaderStyleId};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const CONTENT: Keyword = Keyword::new("Content");
const BLOCK: Keyword = Keyword::new("Block");
const MTEXT: Keyword = Keyword::new("Mtext");
const NONE: Keyword = Keyword::new("None");

/// What the note will be.
#[derive(Clone, Debug, PartialEq)]
enum Note {
    /// Words, asked for after the landing.
    Words,
    /// This block, by its ID and name, at this scale.
    Block(BlockId, String, f64),
    Nothing,
}

#[derive(Debug)]
enum State {
    Arrowhead,
    Content,
    BlockName,
    Landing(Point2),
    Words(Point2, Point2),
}

#[derive(Debug)]
pub(crate) struct MLeaderCommand {
    state: State,
    style: MLeaderStyleId,
    note: Note,
    /// The block scale of the style, for a block chosen here.
    block_scale: f64,
}

impl MLeaderCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Arrowhead,
            style: MLeaderStyleId::FIRST,
            note: Note::Words,
            block_scale: 1.0,
        }
    }

    fn make(&self, tip: Point2, landing: Point2, content: MLeaderContent) -> MLeaderEntity {
        MLeaderEntity::new(tip, landing, content).in_style(self.style)
    }

    /// The note as it is known before any words are typed.
    fn content(&self) -> MLeaderContent {
        match &self.note {
            Note::Block(block, _, scale) => MLeaderContent::Block {
                block: *block,
                scale: *scale,
            },
            Note::Words | Note::Nothing => MLeaderContent::None,
        }
    }

    fn insert(&self, ctx: &mut CommandContext<'_>, mleader: MLeaderEntity) {
        let mut transaction = ctx.transaction();
        match transaction.insert(mleader) {
            Ok(_) => {
                transaction.commit();
            }
            Err(error) => {
                drop(transaction);
                ctx.message(error.to_string());
            }
        }
    }
}

impl Command for MLeaderCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.settings().current_mleader_style;
        let style = ctx.drawing.tables().mleader_style(self.style);
        self.block_scale = style.block_scale;
        // A style that names a block starts with it, when there is one.
        let named = style.block.as_deref().and_then(|name| {
            let id = ctx.drawing.blocks().find(name)?;
            Some(Note::Block(id, name.to_owned(), style.block_scale))
        });
        if let Some(note) = named {
            self.note = note;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Arrowhead => Prompt::new("Specify leader arrowhead location"),
            State::Content => Prompt::new("Enter content type").with_default(match self.note {
                Note::Words => "Mtext",
                Note::Block(..) => "Block",
                Note::Nothing => "None",
            }),
            State::BlockName => match &self.note {
                Note::Block(_, name, _) => {
                    Prompt::new("Enter block name").with_default(name.clone())
                }
                _ => Prompt::new("Enter block name"),
            },
            State::Landing(_) => Prompt::new("Specify leader landing location"),
            State::Words(..) => Prompt::new("Enter the note").with_default("none"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Arrowhead => ExpectedInput::point().with_keywords(&[CONTENT]),
            State::Content => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[BLOCK, MTEXT, NONE])
            }
            State::Landing(_) => ExpectedInput::point().with_direct_distance(),
            State::BlockName | State::Words(..) => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Arrowhead);
        self.state = match (state, input) {
            (State::Arrowhead, CommandInput::Point(tip)) => State::Landing(tip),
            (State::Arrowhead, CommandInput::Keyword(CONTENT)) => State::Content,
            (State::Content, CommandInput::Keyword(BLOCK)) => State::BlockName,
            (State::Content, CommandInput::Keyword(MTEXT)) => {
                self.note = Note::Words;
                State::Arrowhead
            }
            (State::Content, CommandInput::Keyword(NONE)) => {
                self.note = Note::Nothing;
                State::Arrowhead
            }
            (State::Content, _) => State::Arrowhead,
            (State::BlockName, CommandInput::Text(name)) if !name.trim().is_empty() => {
                let name = name.trim();
                match ctx.drawing.blocks().find(name) {
                    Some(id) => {
                        self.note = Note::Block(id, name.to_owned(), self.block_scale);
                        State::Arrowhead
                    }
                    None => {
                        ctx.message(format!("There is no block \"{name}\"."));
                        State::BlockName
                    }
                }
            }
            // Enter keeps the block there is, if there is one.
            (State::BlockName, CommandInput::Text(_) | CommandInput::Accept) => {
                if !matches!(self.note, Note::Block(..)) {
                    ctx.message("No block chosen: the note stays as it was.");
                }
                State::Arrowhead
            }
            (State::Landing(tip), CommandInput::Point(landing)) => {
                if self.note == Note::Words {
                    State::Words(tip, landing)
                } else {
                    let mleader = self.make(tip, landing, self.content());
                    self.insert(ctx, mleader);
                    return Step::Finished;
                }
            }
            (State::Words(tip, landing), CommandInput::Text(words)) if !words.trim().is_empty() => {
                let mleader = self.make(tip, landing, MLeaderContent::Text(words));
                self.insert(ctx, mleader);
                return Step::Finished;
            }
            (State::Words(tip, landing), CommandInput::Text(_) | CommandInput::Accept) => {
                let mleader = self.make(tip, landing, MLeaderContent::None);
                self.insert(ctx, mleader);
                return Step::Finished;
            }
            (State::Arrowhead | State::Landing(_), CommandInput::Accept) => return Step::Finished,
            (state, _) => state,
        };
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Landing(tip) = self.state {
            out.push(OverlayItem::PreviewEntity(
                self.make(tip, cursor, self.content()).into(),
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
