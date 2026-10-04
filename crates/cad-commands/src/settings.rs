//! Drafting settings (DESIGN §5, §13.4).
//!
//! How the drafting aids behave: which object snaps run, the polar angle.
//! Like the toggles, these are session state — not part of the drawing, not
//! saved to DXF, and never undoable.

use crate::SnapModes;

/// The polar increment a new session starts with, in degrees.
pub const DEFAULT_POLAR_INCREMENT: f64 = 90.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DraftingSettings {
    /// Which object snaps run while OSNAP is on.
    pub snap_modes: SnapModes,
    /// The angle between polar tracking directions, in degrees.
    pub polar_increment: f64,
}

impl Default for DraftingSettings {
    fn default() -> Self {
        Self {
            snap_modes: SnapModes::default(),
            polar_increment: DEFAULT_POLAR_INCREMENT,
        }
    }
}
