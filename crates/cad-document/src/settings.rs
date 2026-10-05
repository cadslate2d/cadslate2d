//! Drawing settings: the saved state of a drawing that is neither an entity
//! nor a table row (DESIGN §11, §19.1).
//!
//! They are part of the drawing — written to a file, restored by undo —
//! unlike session state such as the view or the snap modes.

use cad_core::LengthUnit;

use crate::layer::LayerId;
use crate::{Color, Lineweight};

/// The settings of one drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawingSettings {
    /// The layer new objects are put on.
    pub current_layer: LayerId,
    /// The colour new objects are given: `ByLayer` unless set otherwise.
    pub current_color: Color,
    /// The lineweight new objects are given: `ByLayer` unless set otherwise.
    pub current_lineweight: Lineweight,
    /// What the drawing's numbers are numbers of, or `None` for a unitless
    /// drawing. It says what the geometry means and changes none of it: a
    /// line 25 long is 25 long in any unit. A typed length with a unit
    /// suffix is converted into this (DESIGN §14).
    pub unit: Option<LengthUnit>,
}

impl Default for DrawingSettings {
    /// New objects go on layer `0`, coloured by their layer; the drawing
    /// is unitless.
    fn default() -> Self {
        Self {
            current_layer: LayerId::FIRST,
            current_color: Color::ByLayer,
            current_lineweight: Lineweight::ByLayer,
            unit: None,
        }
    }
}

/// A reversible change of the drawing's settings: the whole of them before
/// and after. They are few and small; one shape covers every setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingsChange {
    pub before: DrawingSettings,
    pub after: DrawingSettings,
}

impl SettingsChange {
    /// The change that exactly undoes this one.
    pub fn inverse(&self) -> Self {
        Self {
            before: self.after,
            after: self.before,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_drawing_draws_on_layer_zero() {
        assert_eq!(DrawingSettings::default().current_layer, LayerId::FIRST);
        assert_eq!(DrawingSettings::default().unit, None, "and has no unit");
        assert_eq!(DrawingSettings::default().current_color, Color::ByLayer);
    }

    #[test]
    fn a_settings_change_is_undone_by_swapping_its_ends() {
        let change = SettingsChange {
            before: DrawingSettings::default(),
            after: DrawingSettings {
                current_layer: LayerId::from_raw(3),
                current_color: Color::Index(1),
                current_lineweight: Lineweight::Hundredths(25),
                unit: Some(LengthUnit::Metres),
            },
        };
        assert_eq!(change.inverse().before, change.after);
        assert_eq!(change.inverse().inverse(), change);
    }
}
