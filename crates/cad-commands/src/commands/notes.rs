//! NOTES (ADR-0041 §5, #355): a block of notes on the sheet — its lines,
//! one per prompt, wrapped to a width in paper millimetres, 3.5 mm high,
//! titled, and placed on the sheet where there is room as a view. One undo.

use cad_core::Point2;
use cad_document::{Entity, Layer, MTextEntity, Properties};
use cad_geometry::composition::Role;
use cad_input::ExpectedInput;

use crate::compose::{NewView, compose, sheet_of, views_on};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// The layer notes are written on.
pub const NOTES_LAYER: &str = "NOTES";
/// The notes' text height and their width when none is given, in paper
/// millimetres.
const TEXT: f64 = 3.5;
const WIDTH: f64 = 120.0;

#[derive(Debug, PartialEq)]
enum State {
    Line,
    Width,
    Title,
}

#[derive(Debug)]
pub(crate) struct NotesCommand {
    state: State,
    lines: Vec<String>,
    width: f64,
}

impl NotesCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Line,
            lines: Vec::new(),
            width: WIDTH,
        }
    }

    fn draw(&self, title: String, ctx: &mut CommandContext<'_>) -> Step {
        let Some(sheet) = sheet_of(ctx.drawing) else {
            return Step::Finished;
        };
        let unit = sheet.frame.units_per_mm;
        let taken: Vec<String> = views_on(ctx.drawing, &sheet.group)
            .into_iter()
            .map(|v| v.id)
            .collect();
        let id = (1..)
            .map(|n| {
                if n == 1 {
                    "notes".to_owned()
                } else {
                    format!("notes-{n}")
                }
            })
            .find(|id| !taken.contains(id))
            .expect("a free id");
        let found = ctx.drawing.layers().find(NOTES_LAYER);
        let mut transaction = ctx.transaction();
        let made = (|| -> Result<_, cad_document::TransactionError> {
            let layer = match found {
                Some(id) => id,
                None => transaction.add_layer(Layer::named(NOTES_LAYER))?,
            };
            transaction.insert_with(
                Entity::MText(MTextEntity::new(
                    Point2::ORIGIN,
                    self.width * unit,
                    TEXT * unit,
                    0.0,
                    self.lines.join("\n"),
                )),
                Properties {
                    layer,
                    ..Properties::default()
                },
            )
        })();
        let made = match made {
            Ok(made) => {
                transaction.commit();
                made
            }
            Err(error) => {
                ctx.message(error.to_string());
                return Step::Finished;
            }
        };
        let space = ctx.space;
        let view = NewView {
            id: id.clone(),
            title,
            role: Role::Free,
            entities: vec![made],
            origin: Point2::ORIGIN,
            ratio: None,
            cut: None,
            parts: Vec::new(),
        };
        match compose(ctx.drawing, space, &[view]) {
            Ok(_) => ctx.message(format!("Notes placed on {} as {id}.", sheet.group)),
            Err(reason) => {
                ctx.drawing.rollback_last_in_group();
                ctx.message(reason);
            }
        }
        Step::Finished
    }
}

impl Command for NotesCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if sheet_of(ctx.drawing).is_none() {
            ctx.message("There is no sheet to place notes on: draw one with TITLEBLOCK first.");
            return Step::Finished;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Line if self.lines.is_empty() => {
                Prompt::new("Enter the first line of the notes")
            }
            State::Line => Prompt::new("Enter the next line, or press Enter when all are given"),
            State::Width => {
                Prompt::new("Specify the notes' width in paper millimetres").with_default("120")
            }
            State::Title => Prompt::new("Enter the notes' title").with_default("NOTES"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Width => ExpectedInput::factor(),
            _ => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Line, CommandInput::Text(line)) if !line.trim().is_empty() => {
                self.lines.push(line.trim().to_owned());
            }
            (State::Line, _) => {
                if self.lines.is_empty() {
                    ctx.message("No notes given.");
                    return Step::Finished;
                }
                self.state = State::Width;
            }
            (State::Width, CommandInput::Distance(width)) => {
                if width.is_finite() && width > 0.0 {
                    self.width = width;
                    self.state = State::Title;
                } else {
                    ctx.message("The width must be greater than zero.");
                }
            }
            (State::Width, CommandInput::Accept) => self.state = State::Title,
            (State::Title, CommandInput::Text(title)) if !title.trim().is_empty() => {
                return self.draw(title.trim().to_owned(), ctx);
            }
            (State::Title, _) => return self.draw("NOTES".to_owned(), ctx),
            _ => {}
        }
        Step::Continue
    }
}
