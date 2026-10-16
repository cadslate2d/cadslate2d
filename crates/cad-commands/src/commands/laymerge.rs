//! LAYMRG and LAYDEL (#236): layers folded into another, or deleted with
//! everything on them.
//!
//! The layers are given by picking an object on each, or by `Name` —
//! several, separated by commas — until Enter. `LAYMRG` then asks for the
//! target the same way and moves every object onto it, keeping their
//! other properties, in block definitions too; `LAYDEL` asks to confirm
//! and erases them. Either way the layers go. Layer `0` and the current
//! layer cannot. What is allowed is the layer table's; one undo each.

use cad_document::LayerId;
use cad_input::{ExpectedInput, Keyword};

use super::layer::explain;
use super::modify::counted;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const NAME: Keyword = Keyword::new("Name");
const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");

#[derive(Debug, PartialEq, Eq)]
enum State {
    /// The layers to merge or delete: picks or `Name`.
    Layers,
    LayerNames,
    /// LAYMRG's target layer.
    Target,
    TargetName,
    /// LAYDEL's confirmation.
    Confirm,
}

#[derive(Debug)]
pub(crate) struct LayerMergeCommand {
    delete: bool,
    state: State,
    layers: Vec<LayerId>,
}

impl LayerMergeCommand {
    pub(crate) fn merge() -> Self {
        Self::new(false)
    }

    pub(crate) fn delete() -> Self {
        Self::new(true)
    }

    fn new(delete: bool) -> Self {
        Self {
            delete,
            state: State::Layers,
            layers: Vec::new(),
        }
    }

    fn name(ctx: &CommandContext<'_>, id: LayerId) -> String {
        ctx.drawing
            .layers()
            .get(id)
            .map_or_else(|| id.to_string(), |layer| layer.name.clone())
    }

    /// Adds a layer to those chosen and says which they are.
    fn choose(&mut self, ctx: &mut CommandContext<'_>, id: LayerId) {
        if !self.layers.contains(&id) {
            self.layers.push(id);
        }
        let names: Vec<String> = self
            .layers
            .iter()
            .map(|id| format!("\"{}\"", Self::name(ctx, *id)))
            .collect();
        ctx.message(format!("Selected layers: {}.", names.join(", ")));
    }

    /// The layer an object picked is on.
    fn layer_of(ctx: &CommandContext<'_>, input: &CommandInput) -> Option<LayerId> {
        match input {
            CommandInput::Pick { entity, .. } => ctx.drawing.properties(*entity).map(|p| p.layer),
            _ => None,
        }
    }

    fn finish(&mut self, ctx: &mut CommandContext<'_>, into: Option<LayerId>) -> Step {
        let names: Vec<String> = self.layers.iter().map(|id| Self::name(ctx, *id)).collect();
        let mut transaction = ctx.transaction();
        let done = match into {
            Some(into) => transaction.merge_layers(&self.layers, into),
            None => transaction.delete_layers(&self.layers),
        };
        match done {
            Ok(count) => {
                transaction.commit();
                let layers = counted(names.len(), "layer");
                ctx.message(match into {
                    Some(into) => format!(
                        "{layers} merged into \"{}\": {} moved.",
                        Self::name(ctx, into),
                        counted(count, "object")
                    ),
                    None => format!(
                        "{layers} deleted, with {} on them.",
                        counted(count, "object")
                    ),
                });
            }
            Err(error) => {
                drop(transaction);
                let message = explain(ctx, error);
                ctx.message(message);
            }
        }
        Step::Finished
    }
}

impl Command for LayerMergeCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::Layers if self.delete => Prompt::new("Select object on layer to delete"),
            State::Layers => Prompt::new("Select object on layer to merge"),
            State::LayerNames | State::TargetName => Prompt::new("Enter layer name(s)"),
            State::Target => Prompt::new("Select object on target layer"),
            State::Confirm => {
                Prompt::new("Delete the layers and everything on them?").with_default("No")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Layers | State::Target => ExpectedInput::object().with_keywords(&[NAME]),
            State::LayerNames | State::TargetName => ExpectedInput::text(),
            State::Confirm => {
                ExpectedInput::new(cad_input::InputKind::Nothing).with_keywords(&[YES, NO])
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, &input) {
            (State::Layers | State::Target, CommandInput::Keyword(_)) => {
                self.state = if self.state == State::Layers {
                    State::LayerNames
                } else {
                    State::TargetName
                };
            }
            (State::Layers, CommandInput::Pick { .. }) => {
                if let Some(id) = Self::layer_of(ctx, &input) {
                    self.choose(ctx, id);
                }
            }
            (State::Layers, CommandInput::Accept) => {
                if self.layers.is_empty() {
                    return Step::Finished;
                }
                self.state = if self.delete {
                    State::Confirm
                } else {
                    State::Target
                };
            }
            (State::LayerNames, CommandInput::Text(names)) => {
                for name in names
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                {
                    match ctx.drawing.layers().find(name) {
                        Some(id) => self.choose(ctx, id),
                        None => ctx.message(format!("There is no layer \"{name}\".")),
                    }
                }
                self.state = State::Layers;
            }
            (State::LayerNames | State::TargetName, CommandInput::Accept) => {
                self.state = if self.state == State::LayerNames {
                    State::Layers
                } else {
                    State::Target
                };
            }
            (State::Target, CommandInput::Pick { .. }) => {
                if let Some(into) = Self::layer_of(ctx, &input) {
                    return self.finish(ctx, Some(into));
                }
            }
            (State::TargetName, CommandInput::Text(name)) => {
                match ctx.drawing.layers().find(name.trim()) {
                    Some(into) => return self.finish(ctx, Some(into)),
                    None => {
                        ctx.message(format!("There is no layer \"{}\".", name.trim()));
                        self.state = State::Target;
                    }
                }
            }
            (State::Target, CommandInput::Accept) => return Step::Finished,
            (State::Confirm, CommandInput::Keyword(YES)) => return self.finish(ctx, None),
            (State::Confirm, CommandInput::Keyword(_) | CommandInput::Accept) => {
                ctx.message("Nothing deleted.");
                return Step::Finished;
            }
            _ => {}
        }
        Step::Continue
    }

    /// A layer is merged or deleted whatever its lock.
    fn picks_locked_objects(&self) -> bool {
        true
    }
}
