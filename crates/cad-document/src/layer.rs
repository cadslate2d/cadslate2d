//! Layers: the first of the drawing's tables (DESIGN §11, §32).
//!
//! Layer `0` is the table's permanent first row: it cannot be removed, so
//! `Properties::default()` always names a layer that exists.

use crate::table::{Table, TableId};

/// A layer. What it controls — colour, linetype, lineweight, whether its
/// objects are shown or can be edited — arrives with the layer table's own
/// issue; a layer here is a name that entities can be on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub name: String,
}

impl Layer {
    /// The name of the layer every drawing has.
    pub const ZERO: &'static str = "0";

    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

pub type LayerId = TableId<Layer>;
pub type LayerTable = Table<Layer>;

/// A new drawing's layers: just layer `0`.
pub(crate) fn initial_layers() -> LayerTable {
    Table::starting_with(Layer::named(Layer::ZERO))
}
