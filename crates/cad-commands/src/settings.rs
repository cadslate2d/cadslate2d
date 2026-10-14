//! Drafting settings (DESIGN §5, §13.4).
//!
//! How the drafting aids behave — which object snaps run, the polar angle,
//! the grid spacing — and the values modify commands remember from one use
//! to the next.
//! Like the toggles, these are session state — not part of the drawing, not
//! saved to DXF, and never undoable.

use cad_core::Aabb;

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
    /// The radius FILLET uses. Zero makes a sharp corner.
    pub fillet_radius: f64,
    /// The distances CHAMFER cuts back along the first and the second line.
    pub chamfer_distances: [f64; 2],
    /// How HATCH last filled: a row of `cad_document::hatch_patterns()` —
    /// 0 is `SOLID` — and the pattern's angle (radians) and scale.
    pub hatch_pattern: usize,
    pub hatch_angle: f64,
    pub hatch_scale: f64,
    /// How PLOT last plotted, offered as its defaults (#187).
    pub plot: PlotSettings,
    /// Whether picking a member of a selectable group selects the whole
    /// group (#214): `PICKSTYLE`, flipped by Ctrl+Shift+A.
    pub group_selection: bool,
    /// The shortest and longest arc of a revision cloud (#215).
    pub cloud_arcs: [f64; 2],
    /// The snap grid's spacing across and up (#219), from the origin.
    pub snap_spacing: [f64; 2],
    /// The grid drawn at the snap spacing rather than its own (`GRID Snap`).
    pub grid_follows_snap: bool,
}

/// A paper size PLOT offers, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    A4,
    A3,
    A2,
    A1,
    A0,
    Letter,
    Tabloid,
}

impl Paper {
    pub const ALL: [Self; 7] = [
        Self::A4,
        Self::A3,
        Self::A2,
        Self::A1,
        Self::A0,
        Self::Letter,
        Self::Tabloid,
    ];

    /// The sheet's width and height in millimetres, held upright.
    pub const fn size_mm(self) -> (f64, f64) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::A3 => (297.0, 420.0),
            Self::A2 => (420.0, 594.0),
            Self::A1 => (594.0, 841.0),
            Self::A0 => (841.0, 1189.0),
            Self::Letter => (215.9, 279.4),
            Self::Tabloid => (279.4, 431.8),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::A2 => "A2",
            Self::A1 => "A1",
            Self::A0 => "A0",
            Self::Letter => "Letter",
            Self::Tabloid => "Tabloid",
        }
    }
}

/// How large the drawing is on the paper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlotScale {
    /// As large as the area fits within the margins.
    Fit,
    /// One paper unit to this many drawing units: `Ratio(50.0)` is 1:50. A
    /// unitless drawing is taken in millimetres.
    Ratio(f64),
}

/// What of the drawing goes on the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlotArea {
    /// Everything shown.
    Extents,
    /// This window of model space, clipped at its edge.
    Window(Aabb),
}

/// What PLOT asked for (#187). The page is laid out from these by
/// `cad-plot`; the session keeps the last ones as the next defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotSettings {
    pub paper: Paper,
    pub scale: PlotScale,
    pub area: PlotArea,
    /// Whether lineweights are plotted at their width, or everything at
    /// the default.
    pub lineweights: bool,
    /// Whether everything is plotted black.
    pub monochrome: bool,
}

impl Default for PlotSettings {
    fn default() -> Self {
        Self {
            paper: Paper::A4,
            scale: PlotScale::Fit,
            area: PlotArea::Extents,
            lineweights: true,
            monochrome: false,
        }
    }
}

impl DraftingSettings {
    /// The spacing the grid is drawn at: its own, or the snap spacing
    /// across when it follows snap (#219).
    pub fn grid_spacing_shown(&self) -> f64 {
        if self.grid_follows_snap {
            self.snap_spacing[0]
        } else {
            self.grid_spacing
        }
    }
}

impl Default for DraftingSettings {
    fn default() -> Self {
        Self {
            snap_modes: SnapModes::default(),
            polar_increment: DEFAULT_POLAR_INCREMENT,
            grid_spacing: DEFAULT_GRID_SPACING,
            offset_distance: None,
            fillet_radius: 0.0,
            chamfer_distances: [0.0, 0.0],
            plot: PlotSettings::default(),
            hatch_pattern: 0,
            hatch_angle: 0.0,
            hatch_scale: 1.0,
            group_selection: true,
            cloud_arcs: [7.5, 15.0],
            snap_spacing: [DEFAULT_GRID_SPACING, DEFAULT_GRID_SPACING],
            grid_follows_snap: false,
        }
    }
}
