//! Drawing settings: the saved state of a drawing that is neither an entity
//! nor a table row (DESIGN §11, §19.1).
//!
//! They are part of the drawing — written to a file, restored by undo —
//! unlike session state such as the view or the snap modes.

use crate::layer::LayerId;

/// The settings of one drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawingSettings {
    /// The layer new objects are put on.
    pub current_layer: LayerId,
}

impl Default for DrawingSettings {
    /// New objects go on layer `0`.
    fn default() -> Self {
        Self {
            current_layer: LayerId::FIRST,
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
    }

    #[test]
    fn a_settings_change_is_undone_by_swapping_its_ends() {
        let change = SettingsChange {
            before: DrawingSettings::default(),
            after: DrawingSettings {
                current_layer: LayerId::from_raw(3),
            },
        };
        assert_eq!(change.inverse().before, change.after);
        assert_eq!(change.inverse().inverse(), change);
    }
}
