//! Bringing objects in from another drawing (#222, #223): a file inserted
//! as a block, a block written out as a file, what the clipboard holds.
//!
//! Objects refer to rows — layers, linetypes, text and dimension styles,
//! blocks — by ID, and another drawing's IDs mean nothing here. So every
//! row the objects refer to, directly or through a block, is matched by
//! name: a name this drawing has keeps this drawing's row, as classic CAD
//! does when it inserts or pastes, and a name it lacks brings the other
//! drawing's row along. One transaction, like any other change.

use std::collections::HashMap;

use super::{TransactionBuilder, TransactionError};
use crate::{
    Block, BlockId, DimStyleId, Drawing, Entity, EntityRecord, LayerId, Linetype, LinetypeId,
    Properties, TextStyleId,
};

/// Which of this drawing's rows each of the other drawing's rows became.
#[derive(Debug, Default)]
struct Rows {
    layers: HashMap<LayerId, LayerId>,
    linetypes: HashMap<LinetypeId, LinetypeId>,
    text_styles: HashMap<TextStyleId, TextStyleId>,
    dim_styles: HashMap<DimStyleId, DimStyleId>,
    blocks: HashMap<BlockId, BlockId>,
}

impl TransactionBuilder<'_> {
    /// `records`, which are `source`'s, as this drawing's: every row they
    /// refer to matched by name, the ones this drawing lacks added. The
    /// records come back in the model, for the caller to insert or to put
    /// in a block.
    pub fn bring_in(
        &mut self,
        source: &Drawing,
        records: &[EntityRecord],
    ) -> Result<Vec<EntityRecord>, TransactionError> {
        let mut rows = Rows::default();
        records
            .iter()
            .map(|record| self.bring_record(source, record, &mut rows))
            .collect()
    }

    fn bring_record(
        &mut self,
        source: &Drawing,
        record: &EntityRecord,
        rows: &mut Rows,
    ) -> Result<EntityRecord, TransactionError> {
        let properties = Properties {
            layer: self.bring_layer(source, record.properties.layer, rows)?,
            linetype: self.bring_linetype_of(source, record.properties.linetype, rows)?,
            ..record.properties
        };
        let entity = self.bring_entity(source, &record.entity, rows)?;
        Ok(EntityRecord::new(entity, properties))
    }

    fn bring_entity(
        &mut self,
        source: &Drawing,
        entity: &Entity,
        rows: &mut Rows,
    ) -> Result<Entity, TransactionError> {
        let mut entity = entity.clone();
        match &mut entity {
            Entity::Text(text) => text.style = self.bring_text_style(source, text.style, rows)?,
            Entity::MText(text) => text.style = self.bring_text_style(source, text.style, rows)?,
            Entity::AttributeDefinition(definition) => {
                definition.text.style =
                    self.bring_text_style(source, definition.text.style, rows)?;
            }
            Entity::Dimension(dimension) => {
                dimension.style = self.bring_dim_style(source, dimension.style, rows)?;
            }
            Entity::Leader(leader) => {
                leader.style = self.bring_dim_style(source, leader.style, rows)?;
            }
            Entity::BlockRef(reference) => {
                reference.block = self.bring_block(source, reference.block, rows)?;
            }
            Entity::Line(_)
            | Entity::Circle(_)
            | Entity::Arc(_)
            | Entity::Ellipse(_)
            | Entity::Spline(_)
            | Entity::Polyline(_)
            | Entity::Hatch(_)
            | Entity::Viewport(_)
            | Entity::Wipeout(_)
            | Entity::Point(_)
            | Entity::XLine(_)
            | Entity::Ray(_) => {}
        }
        Ok(entity)
    }

    fn bring_layer(
        &mut self,
        source: &Drawing,
        id: LayerId,
        rows: &mut Rows,
    ) -> Result<LayerId, TransactionError> {
        if let Some(mine) = rows.layers.get(&id) {
            return Ok(*mine);
        }
        let row = source
            .layers()
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLayer(id))?;
        let mine = match self.drawing.layers.find(&row.name) {
            Some(mine) => mine,
            None => {
                let linetype = self.bring_linetype_of(source, row.linetype, rows)?;
                match self.add_layer(crate::Layer { linetype, ..row }) {
                    Ok(mine) | Err(TransactionError::DuplicateLayerName(mine)) => mine,
                    Err(error) => return Err(error),
                }
            }
        };
        rows.layers.insert(id, mine);
        Ok(mine)
    }

    fn bring_linetype_of(
        &mut self,
        source: &Drawing,
        linetype: Linetype,
        rows: &mut Rows,
    ) -> Result<Linetype, TransactionError> {
        let Linetype::Named(id) = linetype else {
            return Ok(linetype);
        };
        if let Some(mine) = rows.linetypes.get(&id) {
            return Ok(Linetype::Named(*mine));
        }
        let row = source
            .linetypes()
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownLinetype(id))?;
        let mine = match self.drawing.linetypes.find(&row.name) {
            Some(mine) => mine,
            None => match self.add_linetype(row) {
                Ok(mine) | Err(TransactionError::DuplicateLinetypeName(mine)) => mine,
                Err(error) => return Err(error),
            },
        };
        rows.linetypes.insert(id, mine);
        Ok(Linetype::Named(mine))
    }

    fn bring_text_style(
        &mut self,
        source: &Drawing,
        id: TextStyleId,
        rows: &mut Rows,
    ) -> Result<TextStyleId, TransactionError> {
        if let Some(mine) = rows.text_styles.get(&id) {
            return Ok(*mine);
        }
        let row = source
            .text_styles()
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownTextStyle(id))?;
        let mine = match self.drawing.text_styles.find(&row.name) {
            Some(mine) => mine,
            None => match self.add_text_style(row) {
                Ok(mine) | Err(TransactionError::DuplicateTextStyleName(mine)) => mine,
                Err(error) => return Err(error),
            },
        };
        rows.text_styles.insert(id, mine);
        Ok(mine)
    }

    fn bring_dim_style(
        &mut self,
        source: &Drawing,
        id: DimStyleId,
        rows: &mut Rows,
    ) -> Result<DimStyleId, TransactionError> {
        if let Some(mine) = rows.dim_styles.get(&id) {
            return Ok(*mine);
        }
        let row = source
            .dim_styles()
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownDimStyle(id))?;
        let mine = match self.drawing.dim_styles.find(&row.name) {
            Some(mine) => mine,
            None => match self.add_dim_style(row) {
                Ok(mine) | Err(TransactionError::DuplicateDimStyleName(mine)) => mine,
                Err(error) => return Err(error),
            },
        };
        rows.dim_styles.insert(id, mine);
        Ok(mine)
    }

    /// A block by name: this drawing's, or the other's brought in with its
    /// contents — the blocks they place first.
    fn bring_block(
        &mut self,
        source: &Drawing,
        id: BlockId,
        rows: &mut Rows,
    ) -> Result<BlockId, TransactionError> {
        if let Some(mine) = rows.blocks.get(&id) {
            return Ok(*mine);
        }
        let row = source
            .blocks()
            .get(id)
            .cloned()
            .ok_or(TransactionError::UnknownBlock(id))?;
        let mine = match self.drawing.blocks.find(&row.name) {
            Some(mine) => mine,
            None => {
                let records = row
                    .entities
                    .iter()
                    .map(|record| self.bring_record(source, record, rows))
                    .collect::<Result<Vec<_>, _>>()?;
                self.define_block(Block::new(row.name.clone(), row.base, records))?
            }
        };
        rows.blocks.insert(id, mine);
        Ok(mine)
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;

    use super::*;
    use crate::{
        BlockReferenceEntity, CircleEntity, Color, DimStyle, DimensionEntity, Layer, LineEntity,
        LinetypeDef, Measure, TextEntity, TextStyle,
    };

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    /// Another drawing: a dashed red layer `walls`, a text style, a
    /// dimension style, and a block `chair` placing a block `seat`, all
    /// used by its model's objects.
    fn source() -> Drawing {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let dashed = tx
            .add_linetype(LinetypeDef::built_in_named("DASHED").unwrap())
            .unwrap();
        let walls = tx
            .add_layer(Layer {
                color: Color::Index(1),
                linetype: Linetype::Named(dashed),
                ..Layer::named("walls")
            })
            .unwrap();
        let notes = tx.add_text_style(TextStyle::named("Notes")).unwrap();
        let arch = tx.add_dim_style(DimStyle::named("Arch")).unwrap();
        let on_walls = Properties::on(walls);
        let seat = tx
            .define_block(Block::new(
                "seat",
                p(0.0, 0.0),
                vec![EntityRecord::new(
                    CircleEntity::new(p(0.0, 0.0), 2.0),
                    on_walls,
                )],
            ))
            .unwrap();
        let chair = tx
            .define_block(Block::new(
                "chair",
                p(0.0, 0.0),
                vec![EntityRecord::new(
                    BlockReferenceEntity::new(seat, p(1.0, 1.0), 1.0, 0.0),
                    Properties::default(),
                )],
            ))
            .unwrap();
        tx.insert_with(LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)), on_walls)
            .unwrap();
        tx.insert(TextEntity {
            style: notes,
            ..TextEntity::new(p(0.0, 5.0), 2.5, 0.0, "Note")
        })
        .unwrap();
        tx.insert(
            DimensionEntity::new(
                Measure::Aligned {
                    a: p(0.0, 0.0),
                    b: p(10.0, 0.0),
                },
                p(5.0, -5.0),
            )
            .in_style(arch),
        )
        .unwrap();
        tx.insert(BlockReferenceEntity::new(chair, p(20.0, 0.0), 1.0, 0.0))
            .unwrap();
        tx.commit();
        drawing
    }

    fn records(drawing: &Drawing) -> Vec<EntityRecord> {
        drawing
            .records()
            .map(|(_, record)| record.clone())
            .collect()
    }

    #[test]
    fn rows_are_matched_by_name_and_what_is_missing_comes_along() {
        let source = source();
        let mut target = Drawing::new();
        let mut tx = target.transaction();
        // The target has its own `WALLS`, blue: it keeps it.
        let walls = tx
            .add_layer(Layer {
                color: Color::Index(5),
                ..Layer::named("WALLS")
            })
            .unwrap();
        tx.commit();

        let mut tx = target.transaction();
        let brought = tx.bring_in(&source, &records(&source)).unwrap();
        for record in &brought {
            tx.insert_with((*record.entity).clone(), record.properties)
                .unwrap();
        }
        tx.commit();
        assert_eq!(
            brought[0].properties.layer, walls,
            "the target's row, by name"
        );
        assert_eq!(target.layers().get(walls).unwrap().color, Color::Index(5));
        assert!(
            target.linetypes().find("DASHED").is_none(),
            "only what is used comes"
        );
        let Entity::Text(text) = &*brought[1].entity else {
            panic!("a text")
        };
        assert_eq!(Some(text.style), target.text_styles().find("Notes"));
        let Entity::Dimension(dimension) = &*brought[2].entity else {
            panic!("a dimension")
        };
        assert_eq!(Some(dimension.style), target.dim_styles().find("Arch"));
        // The block and the block it places, its contents on the target's layer.
        let Entity::BlockRef(reference) = &*brought[3].entity else {
            panic!("a reference")
        };
        let chair = target.blocks().get(reference.block).unwrap();
        assert_eq!(chair.name, "chair");
        let Entity::BlockRef(inner) = &*chair.entities[0].entity else {
            panic!("a nested reference")
        };
        let seat = target.blocks().get(inner.block).unwrap();
        assert_eq!(seat.name, "seat");
        assert_eq!(seat.entities[0].properties.layer, walls);

        // Brought in again: nothing new.
        let (layers, blocks) = (target.layers().len(), target.blocks().len());
        let mut tx = target.transaction();
        tx.bring_in(&source, &records(&source)).unwrap();
        tx.commit();
        assert_eq!(
            (target.layers().len(), target.blocks().len()),
            (layers, blocks)
        );
    }

    #[test]
    fn a_layer_the_target_lacks_comes_with_its_linetype() {
        let source = source();
        let mut target = Drawing::new();
        let mut tx = target.transaction();
        let brought = tx.bring_in(&source, &records(&source)[..1]).unwrap();
        tx.commit();
        let walls = target.layers().find("walls").unwrap();
        assert_eq!(brought[0].properties.layer, walls);
        let layer = target.layers().get(walls).unwrap();
        assert_eq!(layer.color, Color::Index(1));
        assert_eq!(
            layer.linetype,
            Linetype::Named(target.linetypes().find("DASHED").unwrap())
        );
        assert!(target.can_undo(), "one transaction");
    }
}
