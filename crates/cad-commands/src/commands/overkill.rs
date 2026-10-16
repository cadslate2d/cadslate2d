//! OVERKILL (#234): objects drawn on top of each other removed, and
//! collinear lines that overlap or meet merged.
//!
//! The selection, then `Ignore` the properties two objects may differ in
//! — `Layer`, `Color`, `LType`, `LWeight`, switched one at a time — or
//! Enter to go. What is a duplicate is `cad_document::overkill`'s; the
//! command applies its plan in one transaction.

use cad_core::EntityId;
use cad_document::overkill::{Ignore, overkill};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const IGNORE: Keyword = Keyword::new("Ignore");
const LAYER: Keyword = Keyword::new("Layer");
const COLOR: Keyword = Keyword::new("Color");
const LTYPE: Keyword = Keyword::new("LType");
const LWEIGHT: Keyword = Keyword::new("LWeight");
const PROPERTIES: [Keyword; 4] = [LAYER, COLOR, LTYPE, LWEIGHT];

#[derive(Debug, PartialEq, Eq)]
enum State {
    Select,
    Option,
    Ignoring,
}

#[derive(Debug)]
pub(crate) struct OverkillCommand {
    state: State,
    chosen: Vec<EntityId>,
    ignore: Ignore,
}

impl OverkillCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Select,
            chosen: Vec::new(),
            ignore: Ignore::default(),
        }
    }

    fn chosen(&mut self, ctx: &mut CommandContext<'_>) {
        self.chosen = ctx.selection.iter().collect();
        ctx.selection.clear();
        self.state = State::Option;
    }

    /// What is ignored, as the drafter reads it.
    fn ignoring(&self) -> String {
        let names: Vec<&str> = [
            (self.ignore.layer, "layer"),
            (self.ignore.color, "colour"),
            (self.ignore.linetype, "linetype"),
            (self.ignore.lineweight, "lineweight"),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| name)
        .collect();
        if names.is_empty() {
            "Ignoring no properties.".to_owned()
        } else {
            format!("Ignoring {}.", names.join(", "))
        }
    }

    fn run(&self, ctx: &mut CommandContext<'_>) -> Step {
        let plan = overkill(ctx.drawing, &self.chosen, self.ignore);
        if plan.removed.is_empty() {
            ctx.message("No duplicates found.");
            return Step::Finished;
        }
        let merged = plan.lengthened.len();
        let removed = plan.removed.len();
        let mut transaction = ctx.transaction();
        for (id, line) in plan.lengthened {
            transaction
                .replace(id, line)
                .expect("a selected line, replaced once");
        }
        for id in plan.removed {
            transaction
                .remove(id)
                .expect("a selected object, removed once");
        }
        transaction.commit();
        ctx.message(if merged == 0 {
            format!("{} removed.", counted(removed, "duplicate"))
        } else {
            format!(
                "{} removed; {} lengthened to cover them.",
                counted(removed, "object"),
                counted(merged, "line")
            )
        });
        Step::Finished
    }
}

impl Command for OverkillCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if !ctx.selection.is_empty() {
            self.chosen(ctx);
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Select => Prompt::new(SELECT_OBJECTS),
            State::Option => Prompt::new("Enter an option").with_default("done"),
            State::Ignoring => Prompt::new("Property to ignore").with_default("done"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Select => selection_expected(),
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&[IGNORE]),
            State::Ignoring => ExpectedInput::new(InputKind::Nothing).with_keywords(&PROPERTIES),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Select, CommandInput::Accept) => {
                if ctx.selection.is_empty() {
                    ctx.message(NOTHING_SELECTED);
                    return Step::Finished;
                }
                self.chosen(ctx);
            }
            (State::Option, CommandInput::Keyword(_)) => self.state = State::Ignoring,
            (State::Option, CommandInput::Accept) => return self.run(ctx),
            (State::Ignoring, CommandInput::Keyword(keyword)) => {
                let switch = match keyword {
                    LAYER => &mut self.ignore.layer,
                    COLOR => &mut self.ignore.color,
                    LTYPE => &mut self.ignore.linetype,
                    _ => &mut self.ignore.lineweight,
                };
                *switch = !*switch;
                let message = self.ignoring();
                ctx.message(message);
            }
            (State::Ignoring, CommandInput::Accept) => self.state = State::Option,
            _ => {}
        }
        Step::Continue
    }
}
