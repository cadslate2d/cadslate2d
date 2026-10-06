//! The tables an entity's geometry is read through (ADR-0017, ADR-0020).
//!
//! A block reference has no geometry of its own: it is its definition's,
//! read from the block table. A dimension's lines, arrowheads, and text
//! are laid out from its definition points by the style the dimension
//! style table holds. Every question about an entity's geometry — bounds,
//! the distance for picking, what crosses a window, what the renderer
//! draws — takes these tables, so that nothing derived is copied into the
//! entity and nothing in it can go stale (§19.6).

use cad_core::Aabb;

use crate::block::{BlockReferenceEntity, BlockTable, empty_blocks};
use crate::dim_style::{DimStyle, DimStyleId, DimStyleTable, initial_dim_styles};
use crate::properties::Properties;

/// The tables geometry is resolved against.
#[derive(Clone, Copy, Debug)]
pub struct Tables<'a> {
    pub blocks: &'a BlockTable,
    pub dim_styles: &'a DimStyleTable,
}

impl<'a> Tables<'a> {
    /// Tables with nothing but their permanent rows: for geometry that is
    /// known to refer to none — the editing operations' own curves, and
    /// tests of entities that place nothing. A dimension laid out through
    /// them is drawn by `Standard` as a new drawing has it.
    pub fn none() -> Tables<'static> {
        static STYLES: std::sync::OnceLock<DimStyleTable> = std::sync::OnceLock::new();
        Tables {
            blocks: empty_blocks(),
            dim_styles: STYLES.get_or_init(initial_dim_styles),
        }
    }

    /// `blocks` with a new drawing's dimension styles: for code and tests
    /// that have a block table and no drawing.
    pub fn of_blocks(blocks: &BlockTable) -> Tables<'_> {
        Tables {
            blocks,
            dim_styles: Tables::none().dim_styles,
        }
    }

    /// The style a dimension is drawn by: its row, or `Standard` when the
    /// row is missing — which a drawing never allows, but a stray entity
    /// still has to be drawn by something.
    pub fn dim_style(&self, id: DimStyleId) -> &'a DimStyle {
        self.dim_styles
            .get(id)
            .or_else(|| self.dim_styles.get(DimStyleId::FIRST))
            .unwrap_or_else(|| {
                static STANDARD: std::sync::OnceLock<DimStyle> = std::sync::OnceLock::new();
                STANDARD.get_or_init(|| DimStyle::named(DimStyle::STANDARD))
            })
    }

    /// The bounds of a reference: those of its geometry, or a point at
    /// its insertion when it has none.
    pub fn bounds_of(&self, reference: &BlockReferenceEntity) -> Aabb {
        self.blocks
            .flatten(reference, Properties::default())
            .iter()
            .map(|(entity, _)| entity.bounds(*self))
            .reduce(|a, b| a.union(&b))
            .unwrap_or_else(|| Aabb::from_points(reference.insertion, reference.insertion))
    }
}
