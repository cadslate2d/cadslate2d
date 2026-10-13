//! CHPROP: changes the layer, colour, linetype, or lineweight of the
//! selection (DESIGN §32; #141).
//!
//! Like ERASE it takes a selection made beforehand or asks for one; then it
//! stays at its option prompt until Enter, so several properties can be
//! changed in one go. Each option is one transaction over every selected
//! object, and the command is one undo step. Objects on a locked layer are
//! never in the selection, so none is changed. The Properties panel runs
//! this command for every change made in it.

use cad_core::EntityId;
use cad_document::{Color, Linetype, Lineweight, Properties};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::color::NOT_A_COLOR;
use super::linetype::unknown_linetype;
use super::lweight::NOT_A_LINEWEIGHT;
use super::modify::{SELECT_OBJECTS, selection_expected};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const COLOR: Keyword = Keyword::new("Color");
const LAYER: Keyword = Keyword::new("LAyer");
const LTYPE: Keyword = Keyword::new("LType");
const LWEIGHT: Keyword = Keyword::new("LWeight");
const OPTIONS: [Keyword; 4] = [COLOR, LAYER, LTYPE, LWEIGHT];

#[derive(Debug)]
enum State {
    Selecting,
    Option,
    Color,
    Layer,
    Linetype,
    Lineweight,
}

#[derive(Debug)]
pub(crate) struct ChpropCommand {
    state: State,
}

impl ChpropCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Selecting,
        }
    }

    /// Reports the selection and moves on to the options, or finishes if
    /// there is nothing to change.
    fn adopt(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let count = ctx.selection.len();
        if count == 0 {
            ctx.message("No objects selected.");
            return Step::Finished;
        }
        ctx.message(format!("{count} found"));
        self.state = State::Option;
        Step::Continue
    }

    /// Gives every selected object the properties `edit` makes of its own,
    /// in one transaction.
    fn change(ctx: &mut CommandContext<'_>, edit: impl Fn(Properties) -> Properties, what: &str) {
        let records: Vec<(EntityId, Properties)> = ctx
            .selection
            .iter()
            .filter_map(|id| Some((id, ctx.drawing.properties(id)?)))
            .collect();
        let mut transaction = ctx.transaction();
        for (id, properties) in &records {
            transaction
                .set_properties(*id, edit(*properties))
                .expect("selected entities are live, their layers and linetypes exist");
        }
        transaction.commit();
        let objects = if records.len() == 1 {
            "1 object".to_owned()
        } else {
            format!("{} objects", records.len())
        };
        ctx.message(format!("{objects} changed to {what}."));
    }
}

impl Command for ChpropCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.adopt(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Selecting => Prompt::new(SELECT_OBJECTS),
            State::Option => Prompt::new("Enter property to change"),
            State::Color => Prompt::new("Enter new colour (1-255, a name, ByLayer, or ByBlock)"),
            State::Layer => Prompt::new("Enter new layer name"),
            State::Linetype => {
                Prompt::new("Enter new linetype (a loaded name, ByLayer, or ByBlock)")
            }
            State::Lineweight => {
                Prompt::new("Enter new lineweight (mm, Default, ByLayer, or ByBlock)")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Selecting => selection_expected(),
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            _ => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Selecting, CommandInput::Accept) => return self.adopt(ctx),
            (State::Selecting, _) => {}
            (State::Option, CommandInput::Accept) => return Step::Finished,
            (State::Option, CommandInput::Keyword(keyword)) => {
                self.state = match keyword {
                    COLOR => State::Color,
                    LAYER => State::Layer,
                    LTYPE => State::Linetype,
                    _ => State::Lineweight,
                };
            }
            (State::Color, CommandInput::Text(text)) => match Color::parse(&text) {
                Some(color) => {
                    Self::change(ctx, |p| Properties { color, ..p }, &color.describe());
                    self.state = State::Option;
                }
                None => ctx.message(NOT_A_COLOR),
            },
            (State::Layer, CommandInput::Text(text)) => {
                match ctx.drawing.layers().find(text.trim()) {
                    Some(layer) => {
                        let name = ctx
                            .drawing
                            .layers()
                            .get(layer)
                            .map_or_else(String::new, |l| l.name.clone());
                        Self::change(
                            ctx,
                            |p| Properties { layer, ..p },
                            &format!("layer \"{name}\""),
                        );
                        self.state = State::Option;
                    }
                    None => ctx.message(format!("There is no layer \"{}\".", text.trim())),
                }
            }
            (State::Linetype, CommandInput::Text(text)) => {
                match Linetype::parse(&text, ctx.drawing.linetypes()) {
                    Some(linetype) => {
                        let shown = linetype.describe(ctx.drawing.linetypes());
                        Self::change(
                            ctx,
                            |p| Properties { linetype, ..p },
                            &format!("linetype {shown}"),
                        );
                        self.state = State::Option;
                    }
                    None => ctx.message(unknown_linetype(text.trim())),
                }
            }
            (State::Lineweight, CommandInput::Text(text)) => match Lineweight::parse(&text) {
                Some(lineweight) => {
                    Self::change(
                        ctx,
                        |p| Properties { lineweight, ..p },
                        &format!("lineweight {}", lineweight.describe()),
                    );
                    self.state = State::Option;
                }
                None => ctx.message(format!(
                    "{NOT_A_LINEWEIGHT} Or Default, ByLayer, or ByBlock."
                )),
            },
            // Enter at a value prompt goes back to the options.
            (_, CommandInput::Accept) => self.state = State::Option,
            _ => {}
        }
        Step::Continue
    }
}
