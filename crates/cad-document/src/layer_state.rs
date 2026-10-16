//! Saved layer states (#228): the drawing's layers as they were, by name,
//! to switch back to in one step.
//!
//! A layer state is a row of its own table: a name, a description, and
//! for each layer it names, by that layer's name, its switches — on,
//! frozen, locked — its colour, its linetype, by name, and its
//! lineweight. Saving copies the layers as they are; restoring sets each
//! layer the state names that the drawing still has, and leaves the rest
//! as they are. Like every table, it changes in transactions, and a
//! restore is one undo. A new drawing has none.

use crate::layer::Layer;
use crate::names::Named;
use crate::properties::{Color, Lineweight};
use crate::table::{Table, TableChange, TableId};

/// One layer as a state keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedLayer {
    /// The layer's name: a state finds its layers by name, so it outlives
    /// a layer removed and made again.
    pub name: String,
    pub on: bool,
    pub frozen: bool,
    pub locked: bool,
    pub color: Color,
    /// The name of the layer's linetype; a restore leaves the layer's own
    /// when the drawing no longer has one of that name.
    pub linetype: String,
    pub lineweight: Lineweight,
}

impl SavedLayer {
    /// `layer` as it is, with its linetype by `linetype`'s name.
    pub fn of(layer: &Layer, linetype: &str) -> Self {
        Self {
            name: layer.name.clone(),
            on: layer.on,
            frozen: layer.frozen,
            locked: layer.locked,
            color: layer.color,
            linetype: linetype.to_owned(),
            lineweight: layer.lineweight,
        }
    }
}

/// A saved layer state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerState {
    pub name: String,
    /// What the drafter wrote about it; empty by default.
    pub description: String,
    /// Its layers, in the order the drawing listed them when it was saved.
    pub layers: Vec<SavedLayer>,
}

impl LayerState {
    pub fn new(name: impl Into<String>, layers: Vec<SavedLayer>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            layers,
        }
    }

    /// What it keeps of the layer called `name`, whatever its case.
    pub fn layer(&self, name: &str) -> Option<&SavedLayer> {
        self.layers
            .iter()
            .find(|saved| crate::names::same_name(&saved.name, name))
    }
}

impl Named for LayerState {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type LayerStateId = TableId<LayerState>;
pub type LayerStateTable = Table<LayerState>;
/// A reversible change to one saved state: made, removed, or replaced.
pub type LayerStateChange = TableChange<LayerState>;

/// A new drawing's layer states: none.
pub(crate) fn initial_layer_states() -> LayerStateTable {
    Table::empty()
}
