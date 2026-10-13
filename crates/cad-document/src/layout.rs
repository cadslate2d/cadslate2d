//! Layouts (#191; ADR-0029): the spaces of a drawing. Every entity is in
//! one — the model, which is the first row and always there, or a layout,
//! the sheet of paper a file carried or a drafter made. A layout's row is
//! its name and what the file said of it that the drawing does not read:
//! the `LAYOUT` object's groups, the names its paper block had, and the
//! records of its paper the drawing has no entity for, kept as they were
//! and written back (ADR-0025, narrowed).

use cad_core::{Aabb, Point2};

use crate::kept::{RawEntity, RawGroup};
use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

/// A row of the layout table: a space of the drawing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub name: String,
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
    /// The sheet, in paper units, from its origin: the paper width and
    /// height the file gave the layout (44, 45), or an A3 landscape sheet
    /// for a layout made here. What a layout's view fits to.
    pub fn sheet(&self) -> Aabb {
        let value = |code: i32| {
            self.kept
                .iter()
                .find(|(c, _)| *c == code)
                .and_then(|(_, v)| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
        };
        let (width, height) = match (value(44), value(45)) {
            (Some(w), Some(h)) => (w, h),
            _ => (420.0, 297.0),
        };
        Aabb::from_points(Point2::ORIGIN, Point2::new(width, height))
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
        let (width, height) = match (value(44), value(45)) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => (w, h),
            _ => (420.0, 297.0),
        };
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
