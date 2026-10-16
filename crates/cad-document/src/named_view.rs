//! Named views (#235): places in a drawing a drafter returns to.
//!
//! A named view is a row of its own table: a name, the space it is of —
//! the model or a layout — and the rectangle it shows, by its centre and
//! its width and height in drawing units. Saving and deleting one are
//! changes like any table's, in the one history; going to one moves the
//! view, which is the session's and never undone.

use cad_core::{Aabb, Point2, Vec2};

use crate::layout::LayoutId;
use crate::names::Named;
use crate::table::{Table, TableChange, TableId};

#[derive(Clone, Debug, PartialEq)]
pub struct NamedView {
    pub name: String,
    pub space: LayoutId,
    pub center: Point2,
    /// What it shows across and up, in drawing units; positive.
    pub width: f64,
    pub height: f64,
}

impl NamedView {
    /// A view called `name` of `space`, showing `frame`.
    pub fn of(name: impl Into<String>, space: LayoutId, frame: &Aabb) -> Self {
        let size = frame.max() - frame.min();
        Self {
            name: name.into(),
            space,
            center: frame.center(),
            width: size.x,
            height: size.y,
        }
    }

    /// The rectangle it shows.
    pub fn frame(&self) -> Aabb {
        let half = Vec2::new(self.width / 2.0, self.height / 2.0);
        Aabb::from_points(self.center - half, self.center + half)
    }

    /// Whether it shows something: a finite centre, a positive size.
    pub fn is_valid(&self) -> bool {
        self.center.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }
}

impl Named for NamedView {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type NamedViewId = TableId<NamedView>;
pub type NamedViewTable = Table<NamedView>;
/// A reversible change to one named view: saved, deleted, or replaced.
pub type NamedViewChange = TableChange<NamedView>;

/// A new drawing's named views: none.
pub(crate) fn initial_named_views() -> NamedViewTable {
    Table::empty()
}
