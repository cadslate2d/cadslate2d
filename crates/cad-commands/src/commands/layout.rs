//! LAYOUT, MODEL, PSPACE, and MSPACE (#193): the spaces of the drawing —
//! which the session is on, which exist, what they are called — and the
//! model worked through a viewport of a layout.

use cad_document::{Entity, Layout, LayoutId};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, SpaceRequest, Step};

const LIST: Keyword = Keyword::new("?");
const NEW: Keyword = Keyword::new("New");
const RENAME: Keyword = Keyword::new("Rename");
const DELETE: Keyword = Keyword::new("Delete");
const SET: Keyword = Keyword::new("Set");
const OPTIONS: [Keyword; 5] = [LIST, NEW, RENAME, DELETE, SET];

#[derive(Debug)]
enum State {
    Option,
    NewName,
    RenameWhich,
    RenameTo(LayoutId),
    DeleteWhich,
    SetWhich,
}

/// LAYOUT: lists, makes, renames, deletes, and goes to layouts.
#[derive(Debug)]
pub(crate) struct LayoutCommand {
    state: State,
}

impl LayoutCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
        }
    }

    /// The layout named `name`, or the current one for an empty name —
    /// the model included, which some options refuse themselves.
    fn find(ctx: &CommandContext<'_>, name: &str) -> Result<LayoutId, String> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(ctx.space);
        }
        ctx.drawing
            .layouts()
            .find(name)
            .ok_or_else(|| format!("No layout named \"{name}\"."))
    }

    fn current_name(ctx: &CommandContext<'_>) -> String {
        ctx.drawing
            .layouts()
            .get(ctx.space)
            .map_or_else(|| Layout::MODEL.to_owned(), |layout| layout.name.clone())
    }
}

impl Command for LayoutCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Option => Prompt::new("Enter layout option").with_default("Set"),
            State::NewName => Prompt::new("Enter name for the new layout"),
            State::RenameWhich => {
                Prompt::new("Enter name of layout to rename").with_default("the current one")
            }
            State::RenameTo(_) => Prompt::new("Enter new layout name"),
            State::DeleteWhich => {
                Prompt::new("Enter name of layout to delete").with_default("the current one")
            }
            State::SetWhich => Prompt::new("Enter layout to go to"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            _ => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Option, CommandInput::Accept) => self.state = State::SetWhich,
            (State::Option, CommandInput::Keyword(keyword)) => match keyword {
                LIST => {
                    let names: Vec<String> = ctx
                        .drawing
                        .layouts()
                        .iter()
                        .map(|(id, layout)| {
                            if id == ctx.space {
                                format!("{} (current)", layout.name)
                            } else {
                                layout.name.clone()
                            }
                        })
                        .collect();
                    ctx.message(format!("Layouts: {}.", names.join(", ")));
                    return Step::Finished;
                }
                NEW => self.state = State::NewName,
                RENAME => self.state = State::RenameWhich,
                DELETE => self.state = State::DeleteWhich,
                _ => self.state = State::SetWhich,
            },
            (State::NewName, CommandInput::Text(name)) => {
                let mut transaction = ctx.transaction();
                match transaction.add_layout(Layout::named(name.trim())) {
                    Ok(id) => {
                        transaction.commit();
                        ctx.message(format!("Layout \"{}\" made.", name.trim()));
                        ctx.request_space(SpaceRequest::Go(id));
                        return Step::Finished;
                    }
                    Err(error) => ctx.message(format!("Cannot make the layout: {error}.")),
                }
            }
            (State::RenameWhich, CommandInput::Text(name)) => match Self::find(ctx, &name) {
                Ok(id) if id == LayoutId::MODEL => ctx.message("The model keeps its name."),
                Ok(id) => self.state = State::RenameTo(id),
                Err(message) => ctx.message(message),
            },
            (State::RenameWhich, CommandInput::Accept) => {
                if ctx.space == LayoutId::MODEL {
                    ctx.message("The model keeps its name.");
                } else {
                    self.state = State::RenameTo(ctx.space);
                }
            }
            (State::RenameTo(id), CommandInput::Text(name)) => {
                let id = *id;
                let mut transaction = ctx.transaction();
                match transaction.rename_layout(id, name.trim()) {
                    Ok(()) => {
                        transaction.commit();
                        ctx.message(format!("Layout renamed to \"{}\".", name.trim()));
                        return Step::Finished;
                    }
                    Err(error) => ctx.message(format!("Cannot rename the layout: {error}.")),
                }
            }
            (State::DeleteWhich, CommandInput::Text(name)) => {
                return Self::delete(ctx, Self::find(ctx, &name));
            }
            (State::DeleteWhich, CommandInput::Accept) => return Self::delete(ctx, Ok(ctx.space)),
            (State::SetWhich, CommandInput::Text(name)) => match Self::find(ctx, &name) {
                Ok(id) if id == ctx.space => {
                    ctx.message(format!("Already on {}.", Self::current_name(ctx)));
                    return Step::Finished;
                }
                Ok(id) => {
                    ctx.request_space(SpaceRequest::Go(id));
                    return Step::Finished;
                }
                Err(message) => ctx.message(message),
            },
            (State::SetWhich, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }
}

impl LayoutCommand {
    /// Deletes a layout with everything on it. The model stays; deleting
    /// the current layout goes to the model first.
    fn delete(ctx: &mut CommandContext<'_>, which: Result<LayoutId, String>) -> Step {
        let id = match which {
            Ok(id) if id == LayoutId::MODEL => {
                ctx.message("The model cannot be deleted.");
                return Step::Continue;
            }
            Ok(id) => id,
            Err(message) => {
                ctx.message(message);
                return Step::Continue;
            }
        };
        let name = ctx
            .drawing
            .layouts()
            .get(id)
            .map(|layout| layout.name.clone())
            .unwrap_or_default();
        let count = ctx.drawing.entities_in(id).count();
        let mut transaction = ctx.transaction();
        match transaction.remove_layout(id) {
            Ok(()) => {
                transaction.commit();
                ctx.message(format!(
                    "Layout \"{name}\" deleted{}.",
                    match count {
                        0 => String::new(),
                        1 => " with the 1 object on it".to_owned(),
                        n => format!(" with the {n} objects on it"),
                    }
                ));
                if ctx.space == id {
                    ctx.request_space(SpaceRequest::Go(LayoutId::MODEL));
                }
                Step::Finished
            }
            Err(error) => {
                ctx.message(format!("Cannot delete the layout: {error}."));
                Step::Continue
            }
        }
    }
}

/// MODEL: to the model.
#[derive(Debug)]
pub(crate) struct ModelCommand;

impl Command for ModelCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_space(SpaceRequest::Go(LayoutId::MODEL));
        Step::Finished
    }

    immediate_command!();
}

/// PSPACE: back to the paper from the model worked through a viewport.
#[derive(Debug)]
pub(crate) struct PspaceCommand;

impl Command for PspaceCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_space(SpaceRequest::Paper);
        Step::Finished
    }

    immediate_command!();
}

/// MSPACE: the model worked through a viewport of this layout — the one
/// there is, or the one picked.
#[derive(Debug, Default)]
pub(crate) struct MspaceCommand;

impl Command for MspaceCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.space == LayoutId::MODEL {
            ctx.message("MSPACE works through a viewport of a layout: go to a layout first.");
            return Step::Finished;
        }
        let viewports: Vec<_> = ctx
            .drawing
            .shown_in(ctx.space)
            .filter_map(|(id, entity)| matches!(entity, Entity::Viewport(_)).then_some(id))
            .collect();
        match (viewports.first().copied(), viewports.get(1)) {
            (None, _) => {
                ctx.message("This layout has no viewport: MVIEW makes one.");
                Step::Finished
            }
            (Some(only), None) => {
                ctx.request_space(SpaceRequest::ModelThrough(only));
                Step::Finished
            }
            _ => Step::Continue,
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Select viewport")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Object)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Pick { entity, .. } => {
                if matches!(ctx.drawing.entity(entity), Some(Entity::Viewport(_))) {
                    ctx.request_space(SpaceRequest::ModelThrough(entity));
                    Step::Finished
                } else {
                    ctx.message("That is not a viewport.");
                    Step::Continue
                }
            }
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }
}
