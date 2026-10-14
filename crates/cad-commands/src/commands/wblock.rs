//! WBLOCK (#222): a block, or the selection, written as a drawing file of
//! its own.
//!
//! A block by name is written with its base point at the file's origin;
//! the selection, with a base point asked for. The file holds the objects
//! and every row they refer to (`TransactionBuilder::bring_in`), so it
//! opens, or inserts, as they were. The objects stay where they are. The
//! file is written by the host: the command leaves the drawing in the
//! session and asks for it to be saved.

use cad_core::{Point2, Vec2};
use cad_document::{Drawing, EntityRecord};
use cad_input::ExpectedInput;

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, HostRequest, Prompt, Step};

#[derive(Debug)]
enum State {
    /// A block's name, or Enter for the selection.
    Which,
    Select,
    Base,
    /// The file's name, for these records about the origin.
    Name {
        records: Vec<EntityRecord>,
        default: String,
    },
}

#[derive(Debug)]
pub(crate) struct WBlockCommand {
    state: State,
}

/// `records` moved so that `base` is at the origin.
fn about(records: impl Iterator<Item = EntityRecord>, base: Point2) -> Vec<EntityRecord> {
    let offset: Vec2 = Point2::ORIGIN - base;
    records
        .map(|record| EntityRecord::new(record.entity.translated(offset), record.properties))
        .collect()
}

impl WBlockCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Which,
        }
    }

    /// Makes the file's drawing and asks the host to save it.
    fn write(ctx: &mut CommandContext<'_>, records: &[EntityRecord], name: &str) -> Step {
        let mut file = Drawing::new();
        let mut transaction = file.transaction();
        let written = transaction
            .bring_in(ctx.drawing, records)
            .and_then(|brought| {
                brought.into_iter().try_for_each(|record| {
                    transaction
                        .insert_with((*record.entity).clone(), record.properties)
                        .map(|_| ())
                })
            });
        if let Err(error) = written {
            drop(transaction);
            ctx.message(format!("The objects cannot be written: {error}."));
            return Step::Finished;
        }
        transaction.commit();
        let file_name = if name.to_lowercase().ends_with(".dxf") {
            name.to_owned()
        } else {
            format!("{name}.dxf")
        };
        ctx.message(format!(
            "Writing {file_name}: {}.",
            counted(records.len(), "object")
        ));
        *ctx.block_file = Some((file_name, file));
        ctx.request_host(HostRequest::WriteBlock);
        Step::Finished
    }
}

impl Command for WBlockCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if !ctx.selection.is_empty() {
            self.state = State::Base;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Which => {
                Prompt::new("Enter name of block to write").with_default("the selection")
            }
            State::Select => Prompt::new(SELECT_OBJECTS),
            State::Base => Prompt::new("Specify base point"),
            State::Name { default, .. } => {
                Prompt::new("Enter file name").with_default(default.clone())
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Which | State::Name { .. } => ExpectedInput::text(),
            State::Select => selection_expected(),
            State::Base => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Which);
        self.state = match (state, input) {
            (State::Which, CommandInput::Text(name)) => {
                let found = ctx
                    .drawing
                    .blocks()
                    .find(name.trim())
                    .filter(|id| *id != cad_document::BlockId::FIRST)
                    .and_then(|id| ctx.drawing.blocks().get(id))
                    .cloned();
                match found {
                    Some(block) => State::Name {
                        records: about(block.entities.iter().cloned(), block.base),
                        default: block.name.clone(),
                    },
                    None => {
                        ctx.message(format!("There is no block \"{}\".", name.trim()));
                        State::Which
                    }
                }
            }
            (State::Which, CommandInput::Accept) => State::Select,
            (State::Select, CommandInput::Accept) => {
                if ctx.selection.is_empty() {
                    ctx.message(NOTHING_SELECTED);
                    return Step::Finished;
                }
                State::Base
            }
            (State::Base, CommandInput::Point(base)) => {
                let ids: Vec<_> = ctx.selection.iter().collect();
                let records = ids.iter().filter_map(|id| ctx.drawing.record(*id).cloned());
                let records = about(records, base);
                ctx.selection.clear();
                State::Name {
                    records,
                    default: "block".to_owned(),
                }
            }
            (State::Name { records, .. }, CommandInput::Text(name)) if !name.trim().is_empty() => {
                return Self::write(ctx, &records, name.trim());
            }
            (State::Name { records, default }, CommandInput::Accept) => {
                return Self::write(ctx, &records, &default);
            }
            (state, _) => state,
        };
        Step::Continue
    }
}
