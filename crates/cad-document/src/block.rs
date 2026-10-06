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

use std::sync::Arc;

use cad_core::{Aabb, Point2};
use cad_geometry::Transform;

use crate::layer::LayerId;
use crate::names::Named;
use crate::properties::{Color, Linetype, Lineweight};
use crate::table::{Table, TableChange, TableId};
use crate::{Entity, EntityRecord, Properties};

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

pub(crate) use empty_blocks as no_blocks;

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
}

impl BlockReferenceEntity {
    pub fn new(block: BlockId, insertion: Point2, scale: f64, rotation: f64) -> Self {
        Self {
            block,
            insertion,
            scale,
            rotation,
            mirrored: false,
        }
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
    /// Whether `block` holds, directly or through the blocks it holds, a
    /// reference to `target`.
    pub fn reaches(&self, block: BlockId, target: BlockId) -> bool {
        self.reaches_within(block, target, 0)
    }

    fn reaches_within(&self, block: BlockId, target: BlockId, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        self.get(block).is_some_and(|definition| {
            definition
                .references()
                .any(|inner| inner == target || self.reaches_within(inner, target, depth + 1))
        })
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
                entity => out.push((entity, properties)),
            }
        }
    }

    /// The bounds of a reference: those of its geometry, or a point at
    /// its insertion when it has none.
    pub fn bounds_of(&self, reference: &BlockReferenceEntity) -> Aabb {
        self.flatten(reference, Properties::default())
            .iter()
            .map(|(entity, _)| entity.bounds(self))
            .reduce(|a, b| a.union(&b))
            .unwrap_or_else(|| Aabb::from_points(reference.insertion, reference.insertion))
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
        let bounds = table.bounds_of(&reference);
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
            table.bounds_of(&reference),
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
            let (ba, bb) = (a.bounds(&table), b.bounds(&table));
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
