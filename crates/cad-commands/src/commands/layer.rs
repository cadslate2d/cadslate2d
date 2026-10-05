//! LAYER: makes, sets, and switches layers from the command line
//! (DESIGN §5, §32).
//!
//! The command stays at its option prompt until Enter, so several things
//! can be done in one go. Each option is one transaction: with several
//! names, separated by commas, either every layer named is changed or none
//! is, and the refusal says which name was the trouble. What the layer
//! table allows is decided by `cad-document`; this is the conversation.

use cad_document::{Color, Layer, LayerId, TransactionBuilder, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LIST: Keyword = Keyword::new("?");
const NEW: Keyword = Keyword::new("New");
const SET: Keyword = Keyword::new("Set");
const COLOR: Keyword = Keyword::new("Color");
const ON: Keyword = Keyword::new("ON");
const OFF: Keyword = Keyword::new("OFF");
const LOCK: Keyword = Keyword::new("Lock");
const UNLOCK: Keyword = Keyword::new("Unlock");
const RENAME: Keyword = Keyword::new("Rename");
const DELETE: Keyword = Keyword::new("Delete");
const OPTIONS: [Keyword; 10] = [LIST, NEW, SET, COLOR, ON, OFF, LOCK, UNLOCK, RENAME, DELETE];

/// A switch that some options throw on every layer named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Switch {
    On,
    Off,
    Lock,
    Unlock,
}

impl Switch {
    fn apply(self, layer: &mut Layer) {
        match self {
            Self::On => layer.on = true,
            Self::Off => layer.on = false,
            Self::Lock => layer.locked = true,
            Self::Unlock => layer.locked = false,
        }
    }

    /// As in "Enter name(s) of layer(s) to …".
    fn infinitive(self) -> &'static str {
        match self {
            Self::On => "switch on",
            Self::Off => "switch off",
            Self::Lock => "lock",
            Self::Unlock => "unlock",
        }
    }

    /// As in "2 layers …".
    fn done(self) -> &'static str {
        match self {
            Self::On => "switched on",
            Self::Off => "switched off",
            Self::Lock => "locked",
            Self::Unlock => "unlocked",
        }
    }
}

#[derive(Debug)]
enum State {
    Option,
    New,
    Set,
    /// The colour number, then the layers to give it to.
    ColorNumber,
    ColorLayers(u8),
    Switch(Switch),
    RenameFrom,
    RenameTo(LayerId),
    Delete,
}

#[derive(Debug)]
pub(crate) struct LayerCommand {
    state: State,
}

/// "1 layer", "2 layers".
fn layers(count: usize) -> String {
    if count == 1 {
        "1 layer".to_owned()
    } else {
        format!("{count} layers")
    }
}

/// The names in a comma-separated list, trimmed, without repeats.
fn names(text: &str) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for name in text.split(',').map(str::trim) {
        if !names.iter().any(|seen| Layer::same_name(seen, name)) {
            names.push(name);
        }
    }
    names
}

/// The layer called `name`, or the message that there is none.
fn find(ctx: &CommandContext<'_>, name: &str) -> Result<LayerId, String> {
    ctx.drawing
        .layers()
        .find(name)
        .ok_or_else(|| format!("There is no layer \"{name}\"."))
}

/// Why the layer table refused a change, in the user's terms.
fn explain(ctx: &CommandContext<'_>, error: TransactionError) -> String {
    let name = |id: LayerId| {
        ctx.drawing
            .layers()
            .get(id)
            .map_or_else(|| id.to_string(), |layer| layer.name.clone())
    };
    match error {
        TransactionError::BadLayerName(error) => error.to_string(),
        TransactionError::DuplicateLayerName(id) => {
            format!("There is already a layer \"{}\".", name(id))
        }
        TransactionError::PermanentLayer => "Layer \"0\" cannot be renamed or deleted.".to_owned(),
        TransactionError::LayerInUse(id) => format!("Layer \"{}\" has objects on it.", name(id)),
        TransactionError::CurrentLayer(id) => format!(
            "Layer \"{}\" is the current layer: it cannot be switched off or deleted.",
            name(id)
        ),
        TransactionError::LayerIsOff(id) => format!(
            "Layer \"{}\" is off: switch it on before making it current.",
            name(id)
        ),
        other => other.to_string(),
    }
}

impl LayerCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Option,
        }
    }

    fn report_current(ctx: &mut CommandContext<'_>) {
        let current = ctx.drawing.current_layer();
        let name = ctx
            .drawing
            .layers()
            .get(current)
            .map_or_else(String::new, |layer| layer.name.clone());
        ctx.message(format!("Current layer: \"{name}\""));
    }

    /// The `?` option: one line per layer, in the order they were made.
    fn list(ctx: &mut CommandContext<'_>) {
        let current = ctx.drawing.current_layer();
        let lines: Vec<String> = ctx
            .drawing
            .layers()
            .iter()
            .map(|(id, layer)| {
                let color = match layer.color {
                    Color::Index(index) => format!("colour {index}"),
                    Color::ByLayer | Color::ByBlock => "default colour".to_owned(),
                };
                let mut line = format!(
                    "  \"{}\": {color}, {}",
                    layer.name,
                    if layer.on { "on" } else { "off" }
                );
                if layer.locked {
                    line.push_str(", locked");
                }
                if id == current {
                    line.push_str(", current");
                }
                line
            })
            .collect();
        for line in lines {
            ctx.message(line);
        }
    }

    /// Runs one option's changes as one transaction. A refusal abandons all
    /// of them and is explained; nothing is half done.
    fn transact(
        ctx: &mut CommandContext<'_>,
        changes: impl FnOnce(&mut TransactionBuilder<'_>) -> Result<String, TransactionError>,
    ) {
        let mut transaction = ctx.drawing.transaction();
        match changes(&mut transaction) {
            Ok(done) => {
                transaction.commit();
                ctx.message(done);
            }
            Err(error) => {
                drop(transaction);
                let message = explain(ctx, error);
                ctx.message(message);
            }
        }
    }

    /// Replaces each named layer by itself with `edit` applied.
    fn edit_layers(
        ctx: &mut CommandContext<'_>,
        text: &str,
        edit: impl Fn(&mut Layer),
        done: impl FnOnce(usize) -> String,
    ) {
        let found: Result<Vec<LayerId>, String> = names(text)
            .into_iter()
            .map(|name| find(ctx, name))
            .collect();
        let ids = match found {
            Ok(ids) => ids,
            Err(message) => return ctx.message(message),
        };
        let edited: Vec<(LayerId, Layer)> = ids
            .iter()
            .filter_map(|id| {
                let mut layer = ctx.drawing.layers().get(*id)?.clone();
                edit(&mut layer);
                Some((*id, layer))
            })
            .collect();
        Self::transact(ctx, |transaction| {
            let count = edited.len();
            for (id, layer) in edited {
                transaction.replace_layer(id, layer)?;
            }
            Ok(done(count))
        });
    }

    fn text(&mut self, text: &str, ctx: &mut CommandContext<'_>) {
        match &self.state {
            State::New => Self::transact(ctx, |transaction| {
                let names = names(text);
                for name in &names {
                    transaction.add_layer(Layer::named(*name))?;
                }
                Ok(format!("{} created.", layers(names.len())))
            }),
            State::Set => match find(ctx, text) {
                Ok(id) => {
                    let name = ctx
                        .drawing
                        .layers()
                        .get(id)
                        .map_or_else(String::new, |layer| layer.name.clone());
                    Self::transact(ctx, |transaction| {
                        transaction.set_current_layer(id)?;
                        Ok(format!("Current layer: \"{name}\""))
                    });
                }
                Err(message) => ctx.message(message),
            },
            State::ColorLayers(index) => {
                let index = *index;
                Self::edit_layers(
                    ctx,
                    text,
                    |layer| layer.color = Color::Index(index),
                    |count| format!("{} set to colour {index}.", layers(count)),
                );
            }
            State::Switch(switch) => {
                let switch = *switch;
                Self::edit_layers(
                    ctx,
                    text,
                    |layer| switch.apply(layer),
                    |count| format!("{} {}.", layers(count), switch.done()),
                );
            }
            State::RenameFrom => {
                match find(ctx, text) {
                    Ok(id) => self.state = State::RenameTo(id),
                    Err(message) => {
                        ctx.message(message);
                        self.state = State::Option;
                    }
                }
                return;
            }
            State::RenameTo(id) => {
                let id = *id;
                if let Some(mut layer) = ctx.drawing.layers().get(id).cloned() {
                    let old = std::mem::replace(&mut layer.name, text.to_owned());
                    Self::transact(ctx, |transaction| {
                        transaction.replace_layer(id, layer)?;
                        Ok(format!("Layer \"{old}\" renamed \"{text}\"."))
                    });
                }
            }
            State::Delete => {
                let found: Result<Vec<LayerId>, String> = names(text)
                    .into_iter()
                    .map(|name| find(ctx, name))
                    .collect();
                match found {
                    Ok(ids) => Self::transact(ctx, |transaction| {
                        for id in &ids {
                            transaction.remove_layer(*id)?;
                        }
                        Ok(format!("{} deleted.", layers(ids.len())))
                    }),
                    Err(message) => ctx.message(message),
                }
            }
            State::Option | State::ColorNumber => {}
        }
        self.state = State::Option;
    }
}

impl Command for LayerCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        Self::report_current(ctx);
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Option => Prompt::new("Press Enter to finish"),
            State::New => Prompt::new("Enter name(s) for new layer(s)"),
            State::Set => Prompt::new("Enter name of layer to make current"),
            State::ColorNumber => Prompt::new("Enter colour number (1-255)"),
            State::ColorLayers(index) => {
                Prompt::new(format!("Enter name(s) of layer(s) for colour {index}"))
            }
            State::Switch(switch) => Prompt::new(format!(
                "Enter name(s) of layer(s) to {}",
                switch.infinitive()
            )),
            State::RenameFrom => Prompt::new("Enter name of layer to rename"),
            State::RenameTo(_) => Prompt::new("Enter new name"),
            State::Delete => Prompt::new("Enter name(s) of layer(s) to delete"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Option => ExpectedInput::new(InputKind::Nothing).with_keywords(&OPTIONS),
            State::ColorNumber => ExpectedInput::factor(),
            _ => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Option, CommandInput::Accept) => return Step::Finished,
            (State::Option, CommandInput::Keyword(keyword)) => {
                self.state = match keyword {
                    NEW => State::New,
                    SET => State::Set,
                    COLOR => State::ColorNumber,
                    ON => State::Switch(Switch::On),
                    OFF => State::Switch(Switch::Off),
                    LOCK => State::Switch(Switch::Lock),
                    UNLOCK => State::Switch(Switch::Unlock),
                    RENAME => State::RenameFrom,
                    DELETE => State::Delete,
                    _ => {
                        Self::list(ctx);
                        State::Option
                    }
                };
            }
            (State::ColorNumber, CommandInput::Distance(number)) => {
                // A whole number from 1 to 255: an entry of the index palette.
                if number.fract() == 0.0 && (1.0..=255.0).contains(&number) {
                    self.state = State::ColorLayers(number as u8);
                } else {
                    ctx.message("A colour is a whole number from 1 to 255.");
                }
            }
            (_, CommandInput::Text(text)) => self.text(&text, ctx),
            // Enter at any other prompt goes back to the options.
            (_, CommandInput::Accept) => self.state = State::Option,
            _ => {}
        }
        Step::Continue
    }
}
