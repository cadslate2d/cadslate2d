//! Blocks: definitions that hold entities, and the references that place
//! them (DESIGN §11, §19.6, §32; ADR-0017).
//!
//! A *definition* is a named row of the block table holding entity
//! records of its own — geometry with properties — about a base point.
//! Its entities have no IDs: they are not in the drawing, only in the
//! definition, and are addressed by position. A *reference* is an entity
//! of the drawing that places the definition: at an insertion point,
//! turned, scaled uniformly, and perhaps mirrored. Its geometry is the
//! definition's, placed — computed from the table whenever it is asked
//! for, never copied into the reference, so that redefining a block is one
//! change to one row, however many references there are (§19.6).
//!
//! Row 0 is `*Model_Space`: the drawing itself, never referenced, never
//! removed, so that a default `BlockId` always names a row, as for every
//! table (ADR-0013).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[cfg(test)]
use cad_core::Aabb;
use cad_core::Point2;
use cad_geometry::Transform;

use crate::layer::LayerId;
use crate::names::Named;
use crate::properties::{Color, Linetype, Lineweight};
use crate::table::{Table, TableChange, TableId};
#[cfg(test)]
use crate::tables::Tables;
use crate::{AttributeDefinitionEntity, Entity, EntityRecord, Properties};

/// Nesting deeper than this is not followed. Cycles are refused when a
/// block is defined, so this is a guard, not a rule.
const MAX_DEPTH: usize = 32;

/// A block definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub name: String,
    /// The point of the definition that a reference's insertion point
    /// lands on.
    pub base: Point2,
    /// The entities of the definition, with their properties. Shared, so
    /// that a redefinition's history entry does not copy them.
    pub entities: Arc<Vec<EntityRecord>>,
}

impl Block {
    /// The name of the row every drawing has: the drawing itself.
    pub const MODEL_SPACE: &'static str = "*Model_Space";

    pub fn new(name: impl Into<String>, base: Point2, entities: Vec<EntityRecord>) -> Self {
        Self {
            name: name.into(),
            base,
            entities: Arc::new(entities),
        }
    }

    /// The attribute definitions of this block, in order: what a
    /// reference is asked for when placed (#183).
    pub fn attribute_definitions(&self) -> impl Iterator<Item = &AttributeDefinitionEntity> + '_ {
        self.entities
            .iter()
            .filter_map(|record| match &*record.entity {
                Entity::AttributeDefinition(definition) => Some(definition),
                _ => None,
            })
    }

    /// The blocks this definition references directly.
    pub fn references(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.entities
            .iter()
            .filter_map(|record| match &*record.entity {
                Entity::BlockRef(reference) => Some(reference.block),
                _ => None,
            })
    }
}

impl Named for Block {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type BlockId = TableId<Block>;
pub type BlockTable = Table<Block>;
/// A reversible change to one block: defined, removed, or — redefined,
/// renamed — replaced by one row, whatever it holds and however many
/// references place it (§19.6).
pub type BlockChange = TableChange<Block>;

/// A new drawing's blocks: just the drawing itself.
pub(crate) fn initial_blocks() -> BlockTable {
    Table::starting_with(Block::new(Block::MODEL_SPACE, Point2::ORIGIN, Vec::new()))
}

/// A table with no blocks to speak of, for geometry that is known not to
/// be a reference: the editing operations' own curves, and tests of
/// entities that place nothing.
pub fn empty_blocks() -> &'static BlockTable {
    static EMPTY: std::sync::OnceLock<BlockTable> = std::sync::OnceLock::new();
    EMPTY.get_or_init(initial_blocks)
}

/// A value given to a block's attribute by one reference (#183): what the
/// definition's `AttributeDefinition` of this tag shows there. Kept by tag,
/// so a redefinition that keeps the tag keeps the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub tag: String,
    pub value: String,
}

/// A placed block: an entity of the drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockReferenceEntity {
    pub block: BlockId,
    /// Where the definition's base point lands.
    pub insertion: Point2,
    /// Uniform; positive.
    pub scale: f64,
    /// Radians from +X.
    pub rotation: f64,
    /// Reflected across the definition's own X axis before it is turned
    /// and placed: a negative X scale, in DXF's terms. A mirrored
    /// reference stays a reference.
    pub mirrored: bool,
    /// The values of the definition's attributes here, by tag (#183). A
    /// tag the definition has and this list lacks shows the default.
    pub attributes: Vec<Attribute>,
}

impl BlockReferenceEntity {
    pub fn new(block: BlockId, insertion: Point2, scale: f64, rotation: f64) -> Self {
        Self {
            block,
            insertion,
            scale,
            rotation,
            mirrored: false,
            attributes: Vec::new(),
        }
    }

    /// The value this reference gives the attribute `tag`, if any.
    pub fn attribute(&self, tag: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| attribute.tag == tag)
            .map(|attribute| attribute.value.as_str())
    }

    /// This reference with `tag` set to `value`: replaced, or added.
    #[must_use]
    pub fn with_attribute(mut self, tag: &str, value: &str) -> Self {
        match self.attributes.iter_mut().find(|a| a.tag == tag) {
            Some(attribute) => attribute.value = value.to_owned(),
            None => self.attributes.push(Attribute {
                tag: tag.to_owned(),
                value: value.to_owned(),
            }),
        }
        self
    }

    /// Places `entity` of a definition about `base` where this reference
    /// puts it: scaled, mirrored if the reference is, turned, and moved.
    pub fn place(&self, entity: &Entity, base: Point2) -> Entity {
        let local = entity.translated(Point2::ORIGIN - base);
        let scaled = match Transform::scaling(Point2::ORIGIN, self.scale) {
            Some(scaling) => local.transformed(&scaling),
            None => local,
        };
        let faced = if self.mirrored {
            let axis = Transform::mirror(Point2::ORIGIN, Point2::new(1.0, 0.0));
            axis.map_or(scaled.clone(), |mirror| scaled.transformed(&mirror))
        } else {
            scaled
        };
        faced
            .transformed(&Transform::rotation(
                Point2::ORIGIN,
                self.rotation.to_degrees(),
            ))
            .translated(self.insertion - Point2::ORIGIN)
    }

    /// The reference moved, turned, scaled, or mirrored: still a reference.
    /// The rotation is where the definition's X axis points afterwards; a
    /// mirror flips the reference's own mirroring, and the two compose:
    /// what was placed as `turn(mirror(scale(p)))` is placed the same way
    /// with the new angle and the new flag.
    pub fn transformed(&self, transform: &Transform) -> Self {
        Self {
            block: self.block,
            insertion: transform.apply(self.insertion),
            scale: self.scale * transform.scale_factor(),
            rotation: transform.apply_angle(self.rotation),
            mirrored: self.mirrored != transform.reverses_orientation(),
            attributes: self.attributes.clone(),
        }
    }

    /// The properties an entity of the definition is drawn with through
    /// this reference, whose properties are `outer`: layer `0` and every
    /// `ByBlock` mean the reference's own (ADR-0013).
    pub fn resolve(outer: Properties, inner: Properties) -> Properties {
        Properties {
            layer: if inner.layer == LayerId::FIRST {
                outer.layer
            } else {
                inner.layer
            },
            color: match inner.color {
                Color::ByBlock => outer.color,
                other => other,
            },
            linetype: match inner.linetype {
                Linetype::ByBlock => outer.linetype,
                other => other,
            },
            lineweight: match inner.lineweight {
                Lineweight::ByBlock => outer.lineweight,
                other => other,
            },
        }
    }
}

impl BlockTable {
    /// How deep a nest of references is followed: a definition more levels
    /// down than this is not drawn, picked, or snapped to.
    pub const MAX_NESTING: usize = MAX_DEPTH;

    /// How many levels of references lie below `block`: 0 for a definition
    /// of curves alone, 1 for one that places such definitions, and so on,
    /// counted no further than one past [`Self::MAX_NESTING`].
    pub fn nesting_depth(&self, block: BlockId) -> usize {
        let mut known = HashMap::new();
        self.nesting_depth_within(block, &mut known, 0)
    }

    /// Each block's depth is worked out once (`known`), so a definition
    /// placed many times — or one that places itself, in a table not yet
    /// checked for cycles — costs its size, not its size to the power of
    /// the nesting (a fuzzer finding, #177). A block on the path being
    /// followed is a cycle: deeper than the limit, like anything past it.
    fn nesting_depth_within(
        &self,
        block: BlockId,
        known: &mut HashMap<BlockId, Option<usize>>,
        depth: usize,
    ) -> usize {
        if depth > MAX_DEPTH {
            return depth;
        }
        match known.get(&block) {
            Some(Some(found)) => return *found,
            Some(None) => return MAX_DEPTH + 1, // on the path: a cycle
            None => {}
        }
        known.insert(block, None);
        let found = self.get(block).map_or(0, |definition| {
            definition
                .references()
                .map(|inner| 1 + self.nesting_depth_within(inner, known, depth + 1))
                .max()
                .unwrap_or(0)
        });
        known.insert(block, Some(found));
        found
    }

    /// Whether `block` holds, directly or through the blocks it holds, a
    /// reference to `target`. Each block is looked into once.
    pub fn reaches(&self, block: BlockId, target: BlockId) -> bool {
        let mut seen = HashSet::new();
        let mut pending = vec![block];
        while let Some(next) = pending.pop() {
            if !seen.insert(next) {
                continue;
            }
            let Some(definition) = self.get(next) else {
                continue;
            };
            for inner in definition.references() {
                if inner == target {
                    return true;
                }
                pending.push(inner);
            }
        }
        false
    }

    /// Whether `contents` — a definition being made or remade as `id` —
    /// would hold `id` itself, directly or through other blocks.
    pub fn would_cycle(&self, id: BlockId, contents: &Block) -> bool {
        contents
            .references()
            .any(|inner| inner == id || self.reaches(inner, id))
    }

    /// The geometry of a reference in the drawing: every entity of its
    /// definition, nested definitions included, placed where the reference
    /// puts it, each with the properties it is drawn with. A reference to
    /// a block that does not exist has no geometry.
    pub fn flatten(
        &self,
        reference: &BlockReferenceEntity,
        properties: Properties,
    ) -> Vec<(Entity, Properties)> {
        let mut out = Vec::new();
        self.flatten_into(reference, properties, 0, &mut out);
        out
    }

    fn flatten_into(
        &self,
        reference: &BlockReferenceEntity,
        outer: Properties,
        depth: usize,
        out: &mut Vec<(Entity, Properties)>,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        let Some(definition) = self.get(reference.block) else {
            return;
        };
        for record in definition.entities.iter() {
            let placed = reference.place(&record.entity, definition.base);
            let properties = BlockReferenceEntity::resolve(outer, record.properties);
            match placed {
                Entity::BlockRef(inner) => self.flatten_into(&inner, properties, depth + 1, out),
                // An attribute definition shows this reference's value for
                // its tag, or its default, as a text placed with the rest.
                Entity::AttributeDefinition(definition) => {
                    if definition.invisible {
                        continue;
                    }
                    let value = reference
                        .attribute(&definition.tag)
                        .unwrap_or(&definition.text.content);
                    out.push((Entity::Text(definition.text.saying(value)), properties));
                }
                entity => out.push((entity, properties)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use super::*;
    use crate::{CircleEntity, LineEntity};

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    /// A chair: a line from (0,0) to (10,0) and a circle at (5,5), about (0,0).
    fn chair() -> Block {
        Block::new(
            "Chair",
            p(0.0, 0.0),
            vec![
                EntityRecord::new(
                    LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)),
                    Properties::default(),
                ),
                EntityRecord::new(
                    CircleEntity::new(p(5.0, 5.0), 2.0),
                    Properties {
                        color: Color::ByBlock,
                        ..Properties::on(LayerId::from_raw(3))
                    },
                ),
            ],
        )
    }

    fn table_with(blocks: Vec<Block>) -> BlockTable {
        let mut table = initial_blocks();
        for block in blocks {
            let id = table.allocate();
            table.apply(&TableChange::Insert { id, row: block });
        }
        table
    }

    #[test]
    fn a_reference_places_the_definition_scaled_turned_and_moved() {
        let table = table_with(vec![chair()]);
        let reference =
            BlockReferenceEntity::new(BlockId::from_raw(1), p(100.0, 50.0), 2.0, FRAC_PI_2);
        let placed = table.flatten(&reference, Properties::default());
        assert_eq!(placed.len(), 2);
        let Entity::Line(line) = &placed[0].0 else {
            panic!("a line");
        };
        // (10, 0) scaled to (20, 0), turned to (0, 20), moved to (100, 70).
        assert!(close(line.start, p(100.0, 50.0)) && close(line.end, p(100.0, 70.0)));
        let Entity::Circle(circle) = &placed[1].0 else {
            panic!("a circle");
        };
        assert!(close(circle.center, p(90.0, 60.0)));
        assert_eq!(circle.radius, 4.0);
        let bounds = Tables::of_blocks(&table).bounds_of(&reference);
        // The circle, radius 4, at (90, 60); the line up to (100, 70).
        assert!(close(bounds.min(), p(86.0, 50.0)) && close(bounds.max(), p(100.0, 70.0)));
    }

    #[test]
    fn layer_zero_and_by_block_mean_the_references_own() {
        let table = table_with(vec![chair()]);
        let outer = Properties {
            color: Color::Index(1),
            lineweight: Lineweight::Hundredths(50),
            ..Properties::on(LayerId::from_raw(7))
        };
        let reference = BlockReferenceEntity::new(BlockId::from_raw(1), p(0.0, 0.0), 1.0, 0.0);
        let placed = table.flatten(&reference, outer);
        // The line is on layer 0 inside: it takes the reference's layer;
        // its colour is ByLayer, kept.
        assert_eq!(placed[0].1.layer, LayerId::from_raw(7));
        assert_eq!(placed[0].1.color, Color::ByLayer);
        // The circle has its own layer and a ByBlock colour: red.
        assert_eq!(placed[1].1.layer, LayerId::from_raw(3));
        assert_eq!(placed[1].1.color, Color::Index(1));
        assert_eq!(placed[1].1.lineweight, Lineweight::ByLayer);
    }

    #[test]
    fn nested_references_place_through_every_level_and_cycles_are_told() {
        let chair_id = BlockId::from_raw(1);
        let pair = Block::new(
            "Pair",
            p(0.0, 0.0),
            vec![
                EntityRecord::new(
                    BlockReferenceEntity::new(chair_id, p(0.0, 0.0), 1.0, 0.0),
                    Properties::default(),
                ),
                EntityRecord::new(
                    BlockReferenceEntity::new(chair_id, p(20.0, 0.0), 1.0, 0.0),
                    Properties::default(),
                ),
            ],
        );
        let table = table_with(vec![chair(), pair]);
        let pair_id = BlockId::from_raw(2);
        let placed = table.flatten(
            &BlockReferenceEntity::new(pair_id, p(0.0, 100.0), 1.0, 0.0),
            Properties::default(),
        );
        assert_eq!(placed.len(), 4, "two chairs of two entities");
        let Entity::Line(second) = &placed[2].0 else {
            panic!("a line");
        };
        assert!(close(second.start, p(20.0, 100.0)));
        assert!(table.reaches(pair_id, chair_id) && !table.reaches(chair_id, pair_id));
        // A chair_id that held a pair would hold itself.
        let cyclic = Block::new(
            "Chair",
            p(0.0, 0.0),
            vec![EntityRecord::new(
                BlockReferenceEntity::new(pair_id, p(0.0, 0.0), 1.0, 0.0),
                Properties::default(),
            )],
        );
        assert!(table.would_cycle(chair_id, &cyclic));
        assert!(!table.would_cycle(pair_id, &chair()));
        let selfish = Block::new(
            "Me",
            p(0.0, 0.0),
            vec![EntityRecord::new(
                BlockReferenceEntity::new(BlockId::from_raw(3), p(0.0, 0.0), 1.0, 0.0),
                Properties::default(),
            )],
        );
        assert!(table.would_cycle(BlockId::from_raw(3), &selfish));
    }

    #[test]
    fn a_reference_to_nothing_has_no_geometry_and_its_insertion_for_bounds() {
        let table = initial_blocks();
        let reference = BlockReferenceEntity::new(BlockId::from_raw(9), p(3.0, 4.0), 1.0, 0.0);
        assert!(table.flatten(&reference, Properties::default()).is_empty());
        assert_eq!(
            Tables::of_blocks(&table).bounds_of(&reference),
            Aabb::from_points(p(3.0, 4.0), p(3.0, 4.0))
        );
    }

    #[test]
    fn a_mirrored_reference_is_a_reference_with_the_same_geometry_as_mirroring_its_parts() {
        let table = table_with(vec![chair()]);
        let reference = BlockReferenceEntity::new(BlockId::from_raw(1), p(10.0, 5.0), 1.5, 0.4);
        let mirror = Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap();
        let mirrored = reference.transformed(&mirror);
        assert!(mirrored.mirrored);
        assert!(close(mirrored.insertion, p(-10.0, 5.0)));
        let expected: Vec<Entity> = table
            .flatten(&reference, Properties::default())
            .into_iter()
            .map(|(entity, _)| entity.transformed(&mirror))
            .collect();
        let actual: Vec<Entity> = table
            .flatten(&mirrored, Properties::default())
            .into_iter()
            .map(|(entity, _)| entity)
            .collect();
        for (a, b) in expected.iter().zip(&actual) {
            let (ba, bb) = (
                a.bounds(Tables::of_blocks(&table)),
                b.bounds(Tables::of_blocks(&table)),
            );
            assert!(
                close(ba.min(), bb.min()) && close(ba.max(), bb.max()),
                "{a:?} vs {b:?}"
            );
        }
        // Mirrored back, it is upright again.
        let back = mirrored.transformed(&mirror);
        assert!(!back.mirrored && close(back.insertion, reference.insertion));
        assert!((back.rotation - reference.rotation).abs() < 1e-9);
    }
}
