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

use std::fmt;

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
    /// The longest name a layer may have, in characters.
    pub const MAX_NAME: usize = 255;
    /// Characters a name may not contain: the ones DXF symbol names
    /// exclude, which are also the ones that would be ambiguous typed at a
    /// command line (`,` separates names; `*` and `?` are wildcards).
    pub const FORBIDDEN: &'static [char] = &[
        '<', '>', '/', '\\', '"', ':', ';', '?', '*', '|', '=', ',', '`',
    ];
    /// The colour a new layer has: the palette's black-or-white.
    pub const DEFAULT_COLOR: Color = Color::Index(7);

    /// A layer of that name with the settings a new layer has: colour 7,
    /// continuous, the default lineweight, on, unlocked.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: Self::DEFAULT_COLOR,
            linetype: Linetype::Continuous,
            lineweight: Lineweight::Default,
            on: true,
            locked: false,
        }
    }

    /// Whether `name` can be a layer's name.
    pub fn check_name(name: &str) -> Result<(), LayerNameError> {
        if name.is_empty() {
            return Err(LayerNameError::Empty);
        }
        if name.chars().count() > Self::MAX_NAME {
            return Err(LayerNameError::TooLong);
        }
        if name != name.trim() {
            return Err(LayerNameError::Padded);
        }
        match name
            .chars()
            .find(|c| c.is_control() || Self::FORBIDDEN.contains(c))
        {
            Some(found) => Err(LayerNameError::Forbidden(found)),
            None => Ok(()),
        }
    }

    /// Whether two names are the same layer name: case does not count.
    pub fn same_name(a: &str, b: &str) -> bool {
        a.to_lowercase() == b.to_lowercase()
    }
}

/// Why a string cannot be a layer's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerNameError {
    Empty,
    /// More than [`Layer::MAX_NAME`] characters.
    TooLong,
    /// It begins or ends with a space.
    Padded,
    /// It contains this character, which a name may not.
    Forbidden(char),
}

impl fmt::Display for LayerNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "A layer name cannot be empty."),
            Self::TooLong => write!(
                f,
                "A layer name can have at most {} characters.",
                Layer::MAX_NAME
            ),
            Self::Padded => write!(f, "A layer name cannot begin or end with a space."),
            Self::Forbidden(c) if c.is_control() => {
                write!(f, "A layer name cannot contain control characters.")
            }
            Self::Forbidden(c) => write!(f, "A layer name cannot contain \"{c}\"."),
        }
    }
}

pub type LayerId = TableId<Layer>;
pub type LayerTable = Table<Layer>;
/// A reversible change to one layer: made, removed, or — renamed, recoloured,
/// switched off, locked — replaced by itself with one thing different.
pub type LayerChange = TableChange<Layer>;

impl LayerTable {
    /// The layer of that name, whatever its case.
    pub fn find(&self, name: &str) -> Option<LayerId> {
        self.iter()
            .find(|(_, layer)| Layer::same_name(&layer.name, name))
            .map(|(id, _)| id)
    }
}

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
        assert_eq!(layer.linetype, Linetype::Continuous);
        assert_eq!(layer.lineweight, Lineweight::Default);
        assert!(layer.on && !layer.locked);
    }

    #[test]
    fn names_are_checked_against_what_a_file_and_a_command_line_can_hold() {
        for good in [
            "0",
            "walls",
            "Ground floor",
            "A-WALL-EXT",
            "Größe 12",
            "x".repeat(255).as_str(),
        ] {
            assert_eq!(Layer::check_name(good), Ok(()), "{good}");
        }
        assert_eq!(Layer::check_name(""), Err(LayerNameError::Empty));
        assert_eq!(
            Layer::check_name(&"x".repeat(256)),
            Err(LayerNameError::TooLong)
        );
        for padded in [" walls", "walls ", " "] {
            assert_eq!(
                Layer::check_name(padded),
                Err(LayerNameError::Padded),
                "{padded:?}"
            );
        }
        for forbidden in Layer::FORBIDDEN {
            let name = format!("a{forbidden}b");
            assert_eq!(
                Layer::check_name(&name),
                Err(LayerNameError::Forbidden(*forbidden))
            );
        }
        assert_eq!(
            Layer::check_name("a\tb"),
            Err(LayerNameError::Forbidden('\t'))
        );
    }

    #[test]
    fn every_name_error_explains_itself() {
        let messages = [
            LayerNameError::Empty.to_string(),
            LayerNameError::TooLong.to_string(),
            LayerNameError::Padded.to_string(),
            LayerNameError::Forbidden('*').to_string(),
            LayerNameError::Forbidden('\n').to_string(),
        ];
        assert_eq!(messages[3], "A layer name cannot contain \"*\".");
        assert_eq!(messages[1], "A layer name can have at most 255 characters.");
        assert!(messages.iter().all(|message| message.ends_with('.')));
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
        assert!(Layer::same_name("STRAßE", "straße"));
    }
}
