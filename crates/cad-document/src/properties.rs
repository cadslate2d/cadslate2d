//! What an entity carries besides its geometry: the layer it is on, and its
//! colour, linetype, and lineweight (DESIGN §11, §32; ADR-0013).
//!
//! Properties are stored *beside* the geometry, not in it. An [`Entity`]
//! stays pure geometry — the modify operations and everything in
//! `cad-geometry` never see a property — and the drawing keeps the pair as
//! an [`EntityRecord`]. Editing an entity's geometry therefore keeps its
//! properties without anyone having to remember to copy them.
//!
//! `ByLayer` and `ByBlock` are stored as such and resolved only where a
//! concrete value is needed, such as when the scene is built: `ByLayer` to
//! the entity's layer, `ByBlock` to the block reference the entity is
//! drawn through, or to the default outside any block.

use std::sync::Arc;

use crate::layer::LayerId;
use crate::layout::LayoutId;
use crate::linetype::LinetypeId;
use crate::{Entity, SharedEntity};

/// An entity's colour. The index palette and object colours arrive with
/// their own issue; until then only the two indirections are meaningful.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    /// The colour of the entity's layer.
    #[default]
    ByLayer,
    /// The colour of the block reference the entity is drawn through.
    ByBlock,
    /// An entry of the standard index palette, 1 to 255.
    Index(u8),
}

/// An entity's linetype (see `linetype.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Linetype {
    #[default]
    ByLayer,
    ByBlock,
    /// A row of the linetype table; [`Linetype::CONTINUOUS`] is the first.
    Named(LinetypeId),
}

/// An entity's lineweight (see `lineweight.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lineweight {
    #[default]
    ByLayer,
    ByBlock,
    /// The drawing's ordinary width.
    Default,
    /// One of the standard widths, in hundredths of a millimetre.
    Hundredths(u16),
}

/// Everything about an entity that is not its geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Properties {
    pub layer: LayerId,
    pub color: Color,
    pub linetype: Linetype,
    pub lineweight: Lineweight,
}

impl Default for Properties {
    /// On layer `0`, with everything by layer.
    fn default() -> Self {
        Self::on(LayerId::FIRST)
    }
}

impl Properties {
    /// On `layer`, with everything else by layer.
    pub fn on(layer: LayerId) -> Self {
        Self {
            layer,
            color: Color::ByLayer,
            linetype: Linetype::ByLayer,
            lineweight: Lineweight::ByLayer,
        }
    }
}

/// An entity as the drawing stores it: shared geometry and its properties.
/// Cheap to copy; the geometry is not duplicated.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityRecord {
    pub entity: SharedEntity,
    pub properties: Properties,
    /// The space the entity is in: the model, or a layout (#191).
    pub space: LayoutId,
}

impl EntityRecord {
    pub fn new(entity: impl Into<Entity>, properties: Properties) -> Self {
        Self {
            entity: Arc::new(entity.into()),
            properties,
            space: LayoutId::MODEL,
        }
    }

    /// The same properties, in the same space, around different geometry.
    #[must_use]
    pub fn with_entity(&self, entity: impl Into<Entity>) -> Self {
        Self {
            space: self.space,
            ..Self::new(entity, self.properties)
        }
    }

    /// The same geometry — shared, not copied — with different properties.
    #[must_use]
    pub fn with_properties(&self, properties: Properties) -> Self {
        Self {
            entity: Arc::clone(&self.entity),
            properties,
            space: self.space,
        }
    }

    /// The same record in another space (#191).
    #[must_use]
    pub fn in_space(&self, space: LayoutId) -> Self {
        Self {
            entity: Arc::clone(&self.entity),
            properties: self.properties,
            space,
        }
    }
}

/// Geometry with default properties, in the model.
impl From<SharedEntity> for EntityRecord {
    fn from(entity: SharedEntity) -> Self {
        Self {
            entity,
            properties: Properties::default(),
            space: LayoutId::MODEL,
        }
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;

    use super::*;
    use crate::LineEntity;

    #[test]
    fn default_properties_are_layer_zero_and_by_layer() {
        let properties = Properties::default();
        assert_eq!(properties.layer, LayerId::FIRST);
        assert_eq!(
            (properties.color, properties.linetype, properties.lineweight),
            (Color::ByLayer, Linetype::ByLayer, Lineweight::ByLayer)
        );
        assert_eq!(Properties::on(LayerId::FIRST), properties);
    }

    #[test]
    fn restyling_a_record_shares_its_geometry() {
        let line = LineEntity::new(Point2::ORIGIN, Point2::new(1.0, 0.0));
        let record = EntityRecord::new(line, Properties::default());
        let restyled = record.with_properties(Properties {
            color: Color::Index(1),
            ..Properties::default()
        });
        assert!(Arc::ptr_eq(&record.entity, &restyled.entity));
        assert_ne!(record, restyled);

        let moved = record.with_entity(LineEntity::new(Point2::ORIGIN, Point2::new(2.0, 0.0)));
        assert_eq!(moved.properties, record.properties);
        assert!(!Arc::ptr_eq(&record.entity, &moved.entity));
    }
}
