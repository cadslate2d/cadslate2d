//! EXPLODE: taking an entity apart one level (DESIGN §22, §32; ADR-0017).
//!
//! A block reference comes apart into its definition's entities, placed
//! where the reference put them; a reference inside the definition stays
//! a reference, so a nest comes apart one level at a time. A polyline
//! comes apart into its lines and arcs. Each part carries the properties
//! it was drawn with, so the drawing looks the same after as before.

use cad_geometry::Primitive;

use crate::{
    ArcEntity, BlockReferenceEntity, BlockTable, CircleEntity, Entity, LineEntity, Properties,
};

/// Why an entity could not be exploded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplodeError {
    /// The entity has no parts to come apart into; carries the name of
    /// what it is.
    Unsupported(&'static str),
}

impl Entity {
    /// The entity taken apart one level, each part with the properties it
    /// is drawn with.
    ///
    /// A block reference's parts are its definition's entities, placed as
    /// the reference placed them, with layer `0` and `ByBlock` resolved to
    /// the reference's own properties (ADR-0013) — the properties the scene
    /// drew them with — and nested references still references. A
    /// polyline's parts are its segments, as lines and arcs, each with the
    /// polyline's properties. Anything else is refused by name.
    pub fn exploded(
        &self,
        properties: Properties,
        blocks: &BlockTable,
    ) -> Result<Vec<(Entity, Properties)>, ExplodeError> {
        match self {
            Entity::BlockRef(reference) => Ok(blocks
                .get(reference.block)
                .map(|definition| {
                    definition
                        .entities
                        .iter()
                        .map(|record| {
                            (
                                reference.place(&record.entity, definition.base),
                                BlockReferenceEntity::resolve(properties, record.properties),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()),
            Entity::Polyline(polyline) => Ok(polyline
                .primitives()
                .map(|primitive| (entity_of(primitive), properties))
                .collect()),
            other => Err(ExplodeError::Unsupported(other.kind_name())),
        }
    }
}

/// A primitive as an entity of its own.
fn entity_of(primitive: Primitive) -> Entity {
    match primitive {
        Primitive::Segment(segment) => LineEntity::new(segment.start, segment.end).into(),
        Primitive::Circle(circle) => CircleEntity::new(circle.center, circle.radius).into(),
        Primitive::Arc(arc) => ArcEntity::from(arc).into(),
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use cad_core::{Aabb, Point2};
    use cad_geometry::Transform;

    use super::*;
    use crate::{
        Block, BlockId, Color, EntityRecord, LayerId, Lineweight, PolylineEntity, Table,
        TableChange, TextEntity,
    };

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    /// A chair about (0, 0): a seat on layer 0 with a ByBlock colour, and a
    /// back on layer 3 with its own colour; and a pair of chairs 20 apart.
    fn chairs() -> BlockTable {
        let chair = Block::new(
            "Chair",
            Point2::ORIGIN,
            vec![
                EntityRecord::new(
                    LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)),
                    Properties {
                        color: Color::ByBlock,
                        ..Properties::default()
                    },
                ),
                EntityRecord::new(
                    ArcEntity::new(p(5.0, 0.0), 5.0, 0.0, std::f64::consts::PI),
                    Properties {
                        color: Color::Index(5),
                        ..Properties::on(LayerId::from_raw(3))
                    },
                ),
                EntityRecord::new(CircleEntity::new(p(5.0, 10.0), 2.0), Properties::default()),
            ],
        );
        let pair = Block::new(
            "Pair",
            Point2::ORIGIN,
            vec![
                EntityRecord::new(
                    BlockReferenceEntity::new(BlockId::from_raw(1), Point2::ORIGIN, 1.0, 0.0),
                    Properties::default(),
                ),
                EntityRecord::new(
                    BlockReferenceEntity::new(BlockId::from_raw(1), p(20.0, 0.0), 1.0, 0.0),
                    Properties::default(),
                ),
            ],
        );
        let mut table =
            Table::starting_with(Block::new(Block::MODEL_SPACE, Point2::ORIGIN, Vec::new()));
        for block in [chair, pair] {
            let id = table.allocate();
            table.apply(&TableChange::Insert { id, row: block });
        }
        table
    }

    const CHAIR: BlockId = BlockId::from_raw(1);
    const PAIR: BlockId = BlockId::from_raw(2);

    #[test]
    fn a_reference_comes_apart_into_its_definition_placed_with_resolved_properties() {
        let blocks = chairs();
        let outer = Properties {
            color: Color::Index(1),
            lineweight: Lineweight::Hundredths(50),
            ..Properties::on(LayerId::from_raw(7))
        };
        let reference: Entity =
            BlockReferenceEntity::new(CHAIR, p(100.0, 50.0), 2.0, FRAC_PI_2).into();
        let parts = reference.exploded(outer, &blocks).unwrap();
        assert_eq!(parts.len(), 3);
        // The seat, (0,0)-(10,0), scaled to 20, turned up, moved: on the
        // reference's layer and in its colour; its ByLayer lineweight is
        // not ByBlock and stays.
        let Entity::Line(seat) = &parts[0].0 else {
            panic!("a line");
        };
        assert!(close(seat.start, p(100.0, 50.0)) && close(seat.end, p(100.0, 70.0)));
        assert_eq!(
            parts[0].1,
            Properties {
                lineweight: Lineweight::ByLayer,
                ..outer
            }
        );
        // The back keeps its own layer and colour; its lineweight was
        // ByLayer and stays so.
        let Entity::Arc(back) = &parts[1].0 else {
            panic!("an arc");
        };
        assert!(close(back.center, p(100.0, 60.0)));
        assert_eq!(back.radius, 10.0);
        assert_eq!(parts[1].1.layer, LayerId::from_raw(3));
        assert_eq!(parts[1].1.color, Color::Index(5));
        assert_eq!(parts[1].1.lineweight, Lineweight::ByLayer);
    }

    #[test]
    fn a_nest_comes_apart_one_level_at_a_time() {
        let blocks = chairs();
        let reference: Entity = BlockReferenceEntity::new(PAIR, p(0.0, 100.0), 1.0, 0.0).into();
        let parts = reference.exploded(Properties::default(), &blocks).unwrap();
        assert_eq!(parts.len(), 2, "two chairs, still references");
        let Entity::BlockRef(second) = &parts[1].0 else {
            panic!("a reference");
        };
        assert_eq!(second.block, CHAIR);
        assert!(close(second.insertion, p(20.0, 100.0)));
    }

    #[test]
    fn a_polyline_comes_apart_into_its_lines_and_arcs() {
        let polyline: Entity = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)],
            vec![0.0, 1.0],
            true,
        )
        .into();
        let mine = Properties::on(LayerId::from_raw(2));
        let parts = polyline.exploded(mine, crate::empty_blocks()).unwrap();
        let kinds: Vec<&str> = parts.iter().map(|(part, _)| part.kind_name()).collect();
        assert_eq!(kinds, ["line", "arc", "line"]);
        assert!(parts.iter().all(|(_, properties)| *properties == mine));
        let Entity::Arc(arc) = &parts[1].0 else {
            panic!("an arc");
        };
        // A bulge of 1 is a half circle: from (10,0) to (10,10) about (10,5).
        assert!(close(arc.center, p(10.0, 5.0)) && (arc.radius - 5.0).abs() < 1e-9);
    }

    #[test]
    fn what_has_no_parts_is_refused_by_name() {
        let blocks = chairs();
        for (entity, name) in [
            (
                Entity::from(LineEntity::new(p(0.0, 0.0), p(1.0, 0.0))),
                "line",
            ),
            (CircleEntity::new(p(0.0, 0.0), 1.0).into(), "circle"),
            (ArcEntity::new(p(0.0, 0.0), 1.0, 0.0, 1.0).into(), "arc"),
            (TextEntity::new(p(0.0, 0.0), 2.5, 0.0, "a").into(), "text"),
        ] {
            assert_eq!(
                entity.exploded(Properties::default(), &blocks),
                Err(ExplodeError::Unsupported(name))
            );
        }
    }

    #[test]
    fn a_reference_to_nothing_comes_apart_into_nothing() {
        let reference: Entity =
            BlockReferenceEntity::new(BlockId::from_raw(9), p(0.0, 0.0), 1.0, 0.0).into();
        assert_eq!(
            reference.exploded(Properties::default(), crate::empty_blocks()),
            Ok(Vec::new())
        );
    }

    // ── invariant, over any placement (#148) ──

    mod properties {
        use proptest::prelude::*;

        use super::*;

        fn any_reference(block: BlockId) -> impl Strategy<Value = BlockReferenceEntity> {
            (
                -500.0..500f64,
                -500.0..500f64,
                0.1..10f64,
                0.0..std::f64::consts::TAU,
                any::<bool>(),
            )
                .prop_map(move |(x, y, scale, rotation, mirrored)| {
                    BlockReferenceEntity {
                        block,
                        insertion: p(x, y),
                        scale,
                        rotation,
                        mirrored,
                    }
                })
        }

        /// Whether two boxes agree to within a tolerance scaled to where
        /// and how big they are.
        fn same_box(a: &Aabb, b: &Aabb) -> bool {
            let size = a
                .max()
                .x
                .abs()
                .max(a.max().y.abs())
                .max(a.min().x.abs())
                .max(a.min().y.abs());
            let allowed = 1e-9 * (1.0 + size);
            a.min().distance_to(b.min()) <= allowed && a.max().distance_to(b.max()) <= allowed
        }

        proptest! {
            /// EXPLODE: the parts of a reference, however it is placed,
            /// occupy exactly where the reference was — the same box, and
            /// the same curves in the same order — whether the definition
            /// holds curves or references; and taking it apart and
            /// transforming commute.
            #[test]
            fn an_exploded_reference_occupies_where_the_reference_was(
                reference in prop_oneof![any_reference(CHAIR), any_reference(PAIR)],
                dx in -100.0..100f64,
            ) {
                let blocks = chairs();
                let whole: Entity = reference.clone().into();
                let parts = whole.exploded(Properties::default(), &blocks).unwrap();
                prop_assert!(!parts.is_empty());

                let together = parts
                    .iter()
                    .map(|(part, _)| part.bounds(&blocks))
                    .reduce(|a, b| a.union(&b))
                    .unwrap();
                prop_assert!(same_box(&together, &whole.bounds(&blocks)), "{together:?} vs {:?}", whole.bounds(&blocks));

                let mine = whole.primitives(&blocks);
                let theirs: Vec<Primitive> = parts.iter().flat_map(|(part, _)| part.primitives(&blocks)).collect();
                prop_assert_eq!(mine.len(), theirs.len());
                for (a, b) in mine.iter().zip(&theirs) {
                    prop_assert!(same_box(&a.bounds(), &b.bounds()), "{a:?} vs {b:?}");
                }

                // Exploding the moved reference is moving the parts.
                let moved = whole.transformed(&Transform::translation(cad_core::Vec2::new(dx, 0.0)));
                let moved_parts = moved.exploded(Properties::default(), &blocks).unwrap();
                for ((part, _), (moved_part, _)) in parts.iter().zip(&moved_parts) {
                    let expected = part.translated(cad_core::Vec2::new(dx, 0.0)).bounds(&blocks);
                    prop_assert!(same_box(&expected, &moved_part.bounds(&blocks)));
                }
            }
        }
    }
}
