//! Layouts (#191; ADR-0029): the spaces of a drawing. Every entity is in
//! one — the model, which is the first row and always there, or a layout,
//! the sheet of paper a file carried or a drafter made. A layout's row is
//! its name and what the file said of it that the drawing does not read:
//! the `LAYOUT` object's groups, the names its paper block had, and the
//! records of its paper the drawing has no entity for, kept as they were
//! and written back (ADR-0025, narrowed).

use cad_core::{Aabb, Point2, Vec2};

use crate::kept::{RawEntity, RawGroup};
use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

/// A row of the layout table: a space of the drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub name: String,
    /// The sheet's width and height in millimetres, when set here (PLOT
    /// on the layout, #194); otherwise the sheet is what the kept plot
    /// settings say, or A3 landscape. The writer puts it in the plot
    /// settings' paper size (44, 45).
    pub paper_size: Option<Vec2>,
    /// What to plot of it, at what scale, turned how — when set here
    /// (PLOT on the layout, #208); otherwise what the kept plot settings
    /// say, or a whole sheet at one to one. The writer puts it in the
    /// plot settings' groups. Boxed: a layout change holds two rows, and
    /// most rows have none.
    pub plot: Option<Box<LayoutPlot>>,
    /// The `LAYOUT` object as a file held it — plot settings and the
    /// rest, opaque — written back as it was. Empty for a layout made
    /// here, which the writer gives a sheet of its own.
    pub kept: Vec<RawGroup>,
    /// The handle of the paper-space block record and the block's name
    /// in the file, for the writer to tie the kept groups back together.
    /// Empty for a layout made here.
    pub block_record_handle: String,
    pub block_name: String,
    /// Records of the layout's paper that the drawing has no entity for —
    /// a `VIEWPORT` — kept as the file held them and written back.
    pub kept_entities: Vec<RawEntity>,
}

/// What of a layout's paper a plot takes (#208): the plot type its
/// settings name (74).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutArea {
    /// The sheet: the layout as laid out (5), or its limits (2).
    Layout,
    /// What is on the paper (1).
    Extents,
    /// A window of the paper (4), its corners (48, 49 and 140, 141).
    Window(Aabb),
}

/// A layout's plot settings (#208): the area, the scale — paper units per
/// unit of the layout, or a fit — and the quarter turns the sheet is
/// turned counter-clockwise on the page (73: 0 none, 1 a quarter, 2 a
/// half, 3 three quarters).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutPlot {
    pub area: LayoutArea,
    /// `None` to fit; otherwise the paper's millimetres per layout unit.
    pub scale: Option<f64>,
    pub turns: u8,
}

impl Default for LayoutPlot {
    /// A whole sheet at one to one, upright: what a layout made here plots.
    fn default() -> Self {
        Self {
            area: LayoutArea::Layout,
            scale: Some(1.0),
            turns: 0,
        }
    }
}

impl LayoutPlot {
    /// Whether the settings are ones a plot can follow.
    pub fn is_valid(&self) -> bool {
        self.turns < 4
            && self
                .scale
                .is_none_or(|scale| scale.is_finite() && scale > 0.0)
            && match self.area {
                LayoutArea::Window(window) => {
                    let size = window.max() - window.min();
                    size.x > 0.0 && size.y > 0.0
                }
                _ => true,
            }
    }
}

impl Layout {
    /// The name of the model's row, the first of every drawing.
    pub const MODEL: &'static str = "Model";

    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

impl Layout {
    /// A layout with these plot settings.
    #[must_use]
    pub fn with_plot(self, plot: LayoutPlot) -> Self {
        Self {
            plot: Some(Box::new(plot)),
            ..self
        }
    }

    /// The plot settings: as set here, or as the file's kept groups give
    /// them — the plot type (74), the window (48, 49, 140, 141), the scale
    /// (142 over 143, or a fit when the flags (70, bit 16) and the
    /// standard scale (75, 0) say so), the rotation (73) — or a whole
    /// sheet at one to one for a layout made here.
    pub fn plot_settings(&self) -> LayoutPlot {
        if let Some(plot) = self.plot.as_deref().filter(|plot| plot.is_valid()) {
            return *plot;
        }
        let value = |code: i32| {
            self.kept
                .iter()
                .find(|(c, _)| *c == code)
                .and_then(|(_, v)| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        if self.kept.is_empty() {
            return LayoutPlot::default();
        }
        let window = || {
            let (x0, y0) = (value(48)?, value(49)?);
            let (x1, y1) = (value(140)?, value(141)?);
            let window = Aabb::from_points(Point2::new(x0, y0), Point2::new(x1, y1));
            let size = window.max() - window.min();
            (size.x > 0.0 && size.y > 0.0).then_some(window)
        };
        let area = match value(74).map(|v| v as i64) {
            Some(1) => LayoutArea::Extents,
            Some(4) => window().map_or(LayoutArea::Layout, LayoutArea::Window),
            _ => LayoutArea::Layout,
        };
        let flags = value(70).map_or(0, |v| v as i64);
        let standard = value(75).map(|v| v as i64);
        let scale = if flags & 16 != 0 && standard == Some(0) {
            None
        } else {
            match (value(142), value(143)) {
                (Some(paper), Some(drawing)) if paper > 0.0 && drawing > 0.0 => {
                    Some(paper / drawing)
                }
                _ => Some(1.0),
            }
        };
        let turns = value(73).map_or(0, |v| (v as i64).rem_euclid(4) as u8);
        LayoutPlot { area, scale, turns }
    }

    /// A layout with this sheet, in millimetres.
    #[must_use]
    pub fn with_sheet(self, size: Vec2) -> Self {
        Self {
            paper_size: Some(size),
            ..self
        }
    }

    /// Whether a sheet can be this size: finite and positive both ways.
    pub fn sheet_size_allowed(size: Vec2) -> bool {
        size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0
    }

    /// The sheet's width and height in millimetres: as set here, or as
    /// the file's plot settings gave it (44, 45), or A3 landscape for a
    /// layout made here.
    pub fn sheet_size(&self) -> Vec2 {
        if let Some(size) = self
            .paper_size
            .filter(|size| Self::sheet_size_allowed(*size))
        {
            return size;
        }
        let value = |code: i32| {
            self.kept
                .iter()
                .find(|(c, _)| *c == code)
                .and_then(|(_, v)| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
        };
        match (value(44), value(45)) {
            (Some(w), Some(h)) => Vec2::new(w, h),
            _ => Vec2::new(420.0, 297.0),
        }
    }

    /// The sheet, in paper units, from its origin. What a layout's view
    /// fits to, and the page a plot of the layout is (#194).
    pub fn sheet(&self) -> Aabb {
        let size = self.sheet_size();
        Aabb::from_points(Point2::ORIGIN, Point2::new(size.x, size.y))
    }

    /// The sheet's printable area, in paper units: from the plot settings
    /// the file gave the layout — paper width and height (44, 45) less its
    /// margins (40 to 43) — or, for a layout made here, an A3 landscape
    /// sheet with the margins the writer gives it. What `MVIEW Fit` fills.
    pub fn printable_area(&self) -> Aabb {
        let value = |code: i32| {
            self.kept
                .iter()
                .find(|(c, _)| *c == code)
                .and_then(|(_, v)| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let size = self.sheet_size();
        let (width, height) = (size.x, size.y);
        let margin = |code: i32, default: f64| value(code).filter(|m| *m >= 0.0).unwrap_or(default);
        let (left, bottom, right, top) = (
            margin(40, 7.5),
            margin(41, 20.0),
            margin(42, 7.5),
            margin(43, 20.0),
        );
        let min = Point2::new(left, bottom);
        let max = Point2::new(
            (width - right).max(left + 1.0),
            (height - top).max(bottom + 1.0),
        );
        Aabb::from_points(min, max)
    }
}

impl Named for Layout {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type LayoutId = TableId<Layout>;
pub type LayoutTable = Table<Layout>;
/// A reversible change to one layout: made, removed, or renamed.
pub type LayoutChange = TableChange<Layout>;

impl LayoutId {
    /// The model: the space every drawing has, and the one new entities
    /// go into unless told otherwise.
    pub const MODEL: Self = Self::FIRST;
}

/// A new drawing's layouts: just the model.
pub(crate) fn initial_layouts() -> LayoutTable {
    Table::starting_with(Layout::named(Layout::MODEL))
}
