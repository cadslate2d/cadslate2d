//! Layers: the first of the drawing's tables (DESIGN §11, §32).
//!
//! Layer `0` is the table's permanent first row: it cannot be removed or
//! renamed, so `Properties::default()` always names a layer that exists.
//!
//! A layer gives the objects on it a colour, a linetype, and a lineweight —
//! what `ByLayer` resolves to — and two switches:
//!
//! - **off**: its objects are not shown, and are not there for the pointer:
//!   not picked, not selected, not snapped to;
//! - **locked**: its objects are shown and can be snapped to, but nothing
//!   changes them.

use crate::names::Named;
use crate::table::{Table, TableChange, TableId};
use crate::{Color, Linetype, Lineweight};

/// A layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub name: String,
    /// What `ByLayer` means for the colour of objects on this layer. An
    /// indirection here (`ByLayer`, `ByBlock`) means the drawing's default.
    pub color: Color,
    pub linetype: Linetype,
    pub lineweight: Lineweight,
    /// Whether the layer's objects are shown and can be picked.
    pub on: bool,
    /// Whether the layer's objects are protected from change.
    pub locked: bool,
}

impl Layer {
    /// The name of the layer every drawing has.
    pub const ZERO: &'static str = "0";
    /// The colour a new layer has: the palette's black-or-white.
    pub const DEFAULT_COLOR: Color = Color::Index(7);

    /// A layer of that name with the settings a new layer has: colour 7,
    /// continuous, the default lineweight, on, unlocked.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: Self::DEFAULT_COLOR,
            linetype: Linetype::CONTINUOUS,
            lineweight: Lineweight::Default,
            on: true,
            locked: false,
        }
    }
}

impl Named for Layer {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type LayerId = TableId<Layer>;
pub type LayerTable = Table<Layer>;
/// A reversible change to one layer: made, removed, or — renamed, recoloured,
/// switched off, locked — replaced by itself with one thing different.
pub type LayerChange = TableChange<Layer>;

/// A new drawing's layers: just layer `0`.
pub(crate) fn initial_layers() -> LayerTable {
    Table::starting_with(Layer::named(Layer::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_layer_is_on_unlocked_continuous_and_colour_seven() {
        let layer = Layer::named("walls");
        assert_eq!(layer.name, "walls");
        assert_eq!(layer.color, Color::Index(7));
        assert_eq!(layer.linetype, Linetype::CONTINUOUS);
        assert_eq!(layer.lineweight, Lineweight::Default);
        assert!(layer.on && !layer.locked);
    }

    #[test]
    fn a_layer_is_found_by_name_whatever_the_case() {
        let mut table = initial_layers();
        let id = table.allocate();
        table.apply(&TableChange::Insert {
            id,
            row: Layer::named("Walls"),
        });
        assert_eq!(table.find("walls"), Some(id));
        assert_eq!(table.find("WALLS"), Some(id));
        assert_eq!(table.find("0"), Some(LayerId::FIRST));
        assert_eq!(table.find("doors"), None);
    }
}
