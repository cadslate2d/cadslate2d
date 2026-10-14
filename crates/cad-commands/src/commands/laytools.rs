//! The layer tools (#213): layers switched by picking what is on them,
//! or all at once — the classic `LAYISO`, `LAYUNISO`, `LAYOFF`, `LAYON`,
//! `LAYFRZ`, `LAYTHW`, `LAYLCK`, `LAYULK`, and `LAYMCUR`.
//!
//! Each change is one transaction through the layer table, which decides
//! what is allowed — the current layer is never switched off or frozen —
//! and a whole run of picks is one undo. `LAYISO` remembers what it
//! changed in the session, not the drawing, so that `LAYUNISO` puts back
//! only what is still as `LAYISO` left it.

use cad_core::EntityId;
use cad_document::{Layer, LayerId, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::layer::explain;
use super::modify::NOTHING_SELECTED;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// What a pick does to the layer of the object picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Off,
    Freeze,
    Lock,
    Unlock,
    MakeCurrent,
}

impl Tool {
    fn prompt(self) -> &'static str {
        match self {
            Self::Off => "Select an object on the layer to switch off",
            Self::Freeze => "Select an object on the layer to freeze",
            Self::Lock => "Select an object on the layer to lock",
            Self::Unlock => "Select an object on the layer to unlock",
            Self::MakeCurrent => "Select an object whose layer will become current",
        }
    }

    /// As in `Layer "walls" is off.`
    fn done(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Freeze => "frozen",
            Self::Lock => "locked",
            Self::Unlock => "unlocked",
            Self::MakeCurrent => "current",
        }
    }

    /// Whether picks go on until Enter, or one is all.
    fn repeats(self) -> bool {
        matches!(self, Self::Off | Self::Freeze)
    }
}

/// `LAYOFF`, `LAYFRZ`, `LAYLCK`, `LAYULK`, `LAYMCUR`: the layer of each
/// object picked.
#[derive(Debug)]
pub(crate) struct LayerPickCommand {
    tool: Tool,
}

impl LayerPickCommand {
    pub(crate) fn off() -> Self {
        Self { tool: Tool::Off }
    }

    pub(crate) fn freeze() -> Self {
        Self { tool: Tool::Freeze }
    }

    pub(crate) fn lock() -> Self {
        Self { tool: Tool::Lock }
    }

    pub(crate) fn unlock() -> Self {
        Self { tool: Tool::Unlock }
    }

    pub(crate) fn make_current() -> Self {
        Self {
            tool: Tool::MakeCurrent,
        }
    }

    /// Applies the tool to the layer of `entity`, or says why not.
    fn apply(&self, entity: EntityId, ctx: &mut CommandContext<'_>) {
        let Some(id) = ctx.drawing.properties(entity).map(|record| record.layer) else {
            return;
        };
        let Some(mut layer) = ctx.drawing.layers().get(id).cloned() else {
            return;
        };
        let name = layer.name.clone();
        let mut transaction = ctx.transaction();
        let applied = match self.tool {
            Tool::Off => {
                layer.on = false;
                transaction.replace_layer(id, layer)
            }
            Tool::Freeze => {
                layer.frozen = true;
                transaction.replace_layer(id, layer)
            }
            Tool::Lock => {
                layer.locked = true;
                transaction.replace_layer(id, layer)
            }
            Tool::Unlock => {
                layer.locked = false;
                transaction.replace_layer(id, layer)
            }
            Tool::MakeCurrent => transaction.set_current_layer(id),
        };
        match applied {
            Ok(()) => {
                transaction.commit();
                ctx.message(format!("Layer \"{name}\" is {}.", self.tool.done()));
            }
            Err(error) => {
                drop(transaction);
                let message = explain(ctx, error);
                ctx.message(message);
            }
        }
    }
}

impl Command for LayerPickCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(self.tool.prompt())
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::object()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Pick { entity, .. } => {
                self.apply(entity, ctx);
                if self.tool.repeats() {
                    Step::Continue
                } else {
                    Step::Finished
                }
            }
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }

    fn picks_locked_objects(&self) -> bool {
        self.tool == Tool::Unlock
    }
}

/// `LAYON` and `LAYTHW`: every layer on, or every layer thawed.
#[derive(Debug)]
pub(crate) struct LayerAllCommand {
    thaw: bool,
}

impl LayerAllCommand {
    pub(crate) fn on() -> Self {
        Self { thaw: false }
    }

    pub(crate) fn thaw() -> Self {
        Self { thaw: true }
    }
}

impl Command for LayerAllCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let changed: Vec<(LayerId, Layer)> = ctx
            .drawing
            .layers()
            .iter()
            .filter(|(_, layer)| if self.thaw { layer.frozen } else { !layer.on })
            .map(|(id, layer)| {
                let mut layer = layer.clone();
                if self.thaw {
                    layer.frozen = false;
                } else {
                    layer.on = true;
                }
                (id, layer)
            })
            .collect();
        if let Err(message) = replace_all(ctx, changed) {
            ctx.message(message);
        } else {
            ctx.message(if self.thaw {
                "All layers are thawed."
            } else {
                "All layers are on."
            });
        }
        Step::Finished
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing)
    }

    fn step(&mut self, _input: CommandInput, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Finished
    }
}

/// Replaces each layer in one transaction, or none of them and says why.
fn replace_all(ctx: &mut CommandContext<'_>, layers: Vec<(LayerId, Layer)>) -> Result<(), String> {
    let mut transaction = ctx.transaction();
    let applied = layers
        .into_iter()
        .try_for_each(|(id, layer)| transaction.replace_layer(id, layer));
    match applied {
        Ok(()) => {
            transaction.commit();
            Ok(())
        }
        Err(error) => {
            drop(transaction);
            Err(explain(ctx, error))
        }
    }
}

/// What the last `LAYISO` changed: each layer it switched off or locked,
/// with the switches it found and the ones it left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Isolation {
    layers: Vec<(LayerId, Switches, Switches)>,
}

/// A layer's `on` and `locked`, the two switches `LAYISO` throws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Switches {
    on: bool,
    locked: bool,
}

impl Switches {
    fn of(layer: &Layer) -> Self {
        Self {
            on: layer.on,
            locked: layer.locked,
        }
    }

    fn set(self, layer: &mut Layer) {
        layer.on = self.on;
        layer.locked = self.locked;
    }
}

const SETTINGS: Keyword = Keyword::new("Settings");
const OFF: Keyword = Keyword::new("Off");
const LOCK: Keyword = Keyword::new("Lock");

#[derive(Debug, PartialEq, Eq)]
enum IsoState {
    Select,
    Settings,
}

/// `LAYISO`: the layers of the selected objects stay; every other layer is
/// switched off, or locked (`Settings`).
#[derive(Debug)]
pub(crate) struct LayIsoCommand {
    state: IsoState,
    lock: bool,
}

impl LayIsoCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: IsoState::Select,
            lock: false,
        }
    }

    fn isolate(&self, ctx: &mut CommandContext<'_>) -> Step {
        let mut kept: Vec<LayerId> = Vec::new();
        for entity in ctx.selection.iter() {
            if let Some(record) = ctx.drawing.properties(entity)
                && !kept.contains(&record.layer)
            {
                kept.push(record.layer);
            }
        }
        let Some(&first) = kept.first() else {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        };
        let current = ctx.drawing.current_layer();
        let changes: Vec<(LayerId, Layer, Switches, Switches)> = ctx
            .drawing
            .layers()
            .iter()
            .filter(|(id, _)| !kept.contains(id))
            .filter_map(|(id, layer)| {
                let before = Switches::of(layer);
                let after = if self.lock {
                    Switches {
                        locked: true,
                        ..before
                    }
                } else {
                    Switches {
                        on: false,
                        ..before
                    }
                };
                (after != before).then(|| {
                    let mut layer = layer.clone();
                    after.set(&mut layer);
                    (id, layer, before, after)
                })
            })
            .collect();
        let names: Vec<String> = kept
            .iter()
            .filter_map(|id| ctx.drawing.layers().get(*id))
            .map(|layer| format!("\"{}\"", layer.name))
            .collect();
        let mut transaction = ctx.transaction();
        let applied = (|| -> Result<(), TransactionError> {
            // The current layer first, so that the old one can be switched off.
            if !kept.contains(&current) {
                transaction.set_current_layer(first)?;
            }
            for (id, layer, ..) in &changes {
                transaction.replace_layer(*id, layer.clone())?;
            }
            Ok(())
        })();
        match applied {
            Ok(()) => {
                transaction.commit();
                *ctx.isolation = Some(Isolation {
                    layers: changes
                        .into_iter()
                        .map(|(id, _, before, after)| (id, before, after))
                        .collect(),
                });
                ctx.selection.clear();
                ctx.message(match names.as_slice() {
                    [one] => format!("Layer {one} is isolated."),
                    several => format!(
                        "{} layers are isolated: {}.",
                        several.len(),
                        several.join(", ")
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

impl Command for LayIsoCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.isolate(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            IsoState::Select => Prompt::new("Select objects on the layer(s) to be isolated"),
            IsoState::Settings => Prompt::new("Enter setting for layers not isolated")
                .with_default(if self.lock { "Lock" } else { "Off" }),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            IsoState::Select => ExpectedInput::new(InputKind::Selection).with_keywords(&[SETTINGS]),
            IsoState::Settings => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[OFF, LOCK])
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (IsoState::Select, CommandInput::Keyword(SETTINGS)) => {
                self.state = IsoState::Settings;
            }
            (IsoState::Select, CommandInput::Accept) => return self.isolate(ctx),
            (IsoState::Settings, CommandInput::Keyword(keyword)) => {
                self.lock = keyword == LOCK;
                self.state = IsoState::Select;
            }
            (IsoState::Settings, CommandInput::Accept) => self.state = IsoState::Select,
            _ => {}
        }
        Step::Continue
    }
}

/// `LAYUNISO`: the layers the last `LAYISO` switched off or locked, put
/// back as it found them — those still as it left them; a layer changed
/// since keeps its change.
#[derive(Debug)]
pub(crate) struct LayUnisoCommand;

impl Command for LayUnisoCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let Some(isolation) = ctx.isolation.take() else {
            ctx.message("LAYISO has isolated no layers to restore.");
            return Step::Finished;
        };
        let restored: Vec<(LayerId, Layer)> = isolation
            .layers
            .iter()
            .filter_map(|(id, before, after)| {
                let layer = ctx.drawing.layers().get(*id)?;
                (Switches::of(layer) == *after).then(|| {
                    let mut layer = layer.clone();
                    before.set(&mut layer);
                    (*id, layer)
                })
            })
            .collect();
        if restored.is_empty() {
            ctx.message("No layer is as LAYISO left it: nothing to restore.");
            return Step::Finished;
        }
        let count = restored.len();
        let message = match replace_all(ctx, restored) {
            Err(message) => message,
            Ok(()) if count == 1 => "1 layer restored.".to_owned(),
            Ok(()) => format!("{count} layers restored."),
        };
        ctx.message(message);
        Step::Finished
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing)
    }

    fn step(&mut self, _input: CommandInput, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Finished
    }
}
