//! Drafting settings (DESIGN §5, §13.4).
//!
//! How the drafting aids behave — which object snaps run, the polar angle,
//! the grid spacing — and the values modify commands remember from one use
//! to the next.
//! Like the toggles, these are session state — not part of the drawing, not
//! saved to DXF, and never undoable.

use crate::SnapModes;

/// The polar increment a new session starts with, in degrees.
pub const DEFAULT_POLAR_INCREMENT: f64 = 90.0;
/// The grid spacing a new session starts with, in drawing units.
pub const DEFAULT_GRID_SPACING: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DraftingSettings {
    /// Which object snaps run while OSNAP is on.
    pub snap_modes: SnapModes,
    /// The angle between polar tracking directions, in degrees.
    pub polar_increment: f64,
    /// The distance between grid lines, in drawing units. The display may
    /// show every 2nd, 5th, 10th… line when zoomed out, never a finer grid.
    pub grid_spacing: f64,
    /// The distance OFFSET last used, offered as its default.
    pub offset_distance: Option<f64>,
}

impl Default for DraftingSettings {
    fn default() -> Self {
        Self {
            snap_modes: SnapModes::default(),
            polar_increment: DEFAULT_POLAR_INCREMENT,
            grid_spacing: DEFAULT_GRID_SPACING,
            offset_distance: None,
        }
    }
}
