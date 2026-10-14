//! Transient edits of existing entities (DESIGN §17).
//!
//! While the pointer moves, an edit exists only as a description in the
//! overlay: the canonical document, its spatial index, and the committed
//! render scene are untouched. On commit, the edit becomes exactly one
//! transaction. Cancelling needs no cleanup, because nothing was changed.

use cad_core::EntityId;
use cad_document::{Drawing, Entity};
use cad_geometry::Transform;

use crate::{OverlayItem, Selection};

/// The entities an edit command is working on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransientEdit {
    sources: Vec<EntityId>,
}

impl TransientEdit {
    /// An edit of everything currently selected.
    pub fn of(selection: &Selection) -> Self {
        Self {
            sources: selection.iter().collect(),
        }
    }

    /// An edit of exactly these entities.
    pub fn of_ids(sources: Vec<EntityId>) -> Self {
        Self { sources }
    }

    /// The entities being edited, in ID order. Snapping must skip these while
    /// the edit is in progress (§18.2).
    pub fn sources(&self) -> &[EntityId] {
        &self.sources
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Shows the sources where `transform` would put them.
    pub fn preview(&self, transform: Transform, out: &mut Vec<OverlayItem>) {
        if !self.sources.is_empty() {
            out.push(OverlayItem::PreviewTransformed {
                ids: self.sources.clone(),
                transform,
            });
        }
    }

    /// Transforms the sources in place, in one transaction. IDs are unchanged.
    pub fn commit(&self, drawing: &mut Drawing, transform: Transform) {
        let changed = self.transformed(drawing, transform);
        let mut transaction = drawing.transaction();
        for (id, entity) in changed {
            transaction
                .replace(id, entity)
                .expect("sources are live and appear once");
        }
        transaction.commit();
    }

    /// Inserts transformed copies of the sources, in one transaction, leaving
    /// the sources as they are. Each copy has its source's properties.
    /// Returns the new IDs.
    pub fn commit_copies(&self, drawing: &mut Drawing, transform: Transform) -> Vec<EntityId> {
        self.commit_copies_under(drawing, &[transform])
    }

    /// A copy of every source under every transform — an array (#199) —
    /// in one transaction, transform by transform. Returns the new IDs.
    pub fn commit_copies_under(
        &self,
        drawing: &mut Drawing,
        transforms: &[Transform],
    ) -> Vec<EntityId> {
        let copies: Vec<(EntityId, Entity)> = transforms
            .iter()
            .flat_map(|transform| self.transformed(drawing, *transform))
            .collect();
        let mut transaction = drawing.transaction();
        let ids = copies
            .into_iter()
            .map(|(source, entity)| {
                transaction
                    .insert_like(source, entity)
                    .expect("sources are live")
            })
            .collect();
        transaction.commit();
        ids
    }

    /// Each live source with its transformed geometry.
    fn transformed(&self, drawing: &Drawing, transform: Transform) -> Vec<(EntityId, Entity)> {
        self.sources
            .iter()
            .filter_map(|&id| Some((id, drawing.entity(id)?.transformed(&transform))))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use cad_core::{Point2, Vec2};
    use cad_document::{CircleEntity, LineEntity};

    use super::*;

    fn by(x: f64, y: f64) -> Transform {
        Transform::translation(Vec2::new(x, y))
    }

    fn drawing() -> (Drawing, Selection) {
        let mut drawing = Drawing::new();
        let mut transaction = drawing.transaction();
        let line = transaction
            .insert(LineEntity::new(
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
            ))
            .unwrap();
        let circle = transaction
            .insert(CircleEntity::new(Point2::new(5.0, 5.0), 2.0))
            .unwrap();
        transaction
            .insert(LineEntity::new(
                Point2::new(100.0, 100.0),
                Point2::new(101.0, 100.0),
            ))
            .unwrap();
        transaction.commit();

        let mut selection = Selection::default();
        selection.add(line);
        selection.add(circle);
        (drawing, selection)
    }

    #[test]
    fn previewing_describes_the_edit_and_changes_nothing() {
        let (drawing, selection) = drawing();
        let revision = drawing.revision();
        let edit = TransientEdit::of(&selection);

        let mut overlay = Vec::new();
        for step in 0..1000 {
            overlay.clear();
            edit.preview(by(f64::from(step), 1.0), &mut overlay);
        }

        assert_eq!(
            overlay,
            vec![OverlayItem::PreviewTransformed {
                ids: edit.sources().to_vec(),
                transform: by(999.0, 1.0)
            }]
        );
        assert_eq!(
            drawing.revision(),
            revision,
            "a thousand previews, no document change"
        );
    }

    #[test]
    fn committing_a_translation_is_one_transaction_and_keeps_ids() {
        let (mut drawing, selection) = drawing();
        let edit = TransientEdit::of(&selection);
        let ids = edit.sources().to_vec();

        edit.commit(&mut drawing, by(100.0, 0.0));
        assert_eq!(
            drawing
                .entity(ids[0])
                .unwrap()
                .bounds(cad_document::Tables::none())
                .min(),
            Point2::new(100.0, 0.0)
        );
        assert_eq!(
            drawing
                .entity(ids[1])
                .unwrap()
                .bounds(cad_document::Tables::none())
                .min(),
            Point2::new(103.0, 3.0)
        );
        assert_eq!(drawing.len(), 3);

        drawing.undo().unwrap();
        assert_eq!(
            drawing
                .entity(ids[0])
                .unwrap()
                .bounds(cad_document::Tables::none())
                .min(),
            Point2::new(0.0, 0.0)
        );
        assert!(!drawing.can_undo() || drawing.undo().is_some());
    }

    #[test]
    fn any_transform_commits_as_one_undoable_transaction() {
        let (mut drawing, selection) = drawing();
        let edit = TransientEdit::of(&selection);
        let ids = edit.sources().to_vec();
        let before = drawing.entity(ids[0]).unwrap().clone();

        edit.commit(&mut drawing, Transform::rotation(Point2::ORIGIN, 90.0));
        assert_eq!(
            drawing.entity(ids[0]).unwrap(),
            &Entity::from(LineEntity::new(Point2::ORIGIN, Point2::new(0.0, 10.0)))
        );
        assert_eq!(
            drawing.entity(ids[1]).unwrap(),
            &Entity::from(CircleEntity::new(Point2::new(-5.0, 5.0), 2.0))
        );

        drawing.undo().unwrap();
        assert_eq!(
            drawing.entity(ids[0]).unwrap(),
            &before,
            "one undo restores both"
        );
        assert_eq!(
            drawing.entity(ids[1]).unwrap(),
            &Entity::from(CircleEntity::new(Point2::new(5.0, 5.0), 2.0))
        );
    }

    #[test]
    fn committing_copies_inserts_new_entities_and_leaves_the_sources() {
        let (mut drawing, selection) = drawing();
        let edit = TransientEdit::of(&selection);

        let copies = edit.commit_copies(&mut drawing, by(0.0, 50.0));
        assert_eq!(copies.len(), 2);
        assert_eq!(drawing.len(), 5);
        assert!(
            copies.iter().all(|id| !edit.sources().contains(id)),
            "copies get fresh IDs"
        );
        assert_eq!(
            drawing
                .entity(edit.sources()[0])
                .unwrap()
                .bounds(cad_document::Tables::none())
                .min(),
            Point2::new(0.0, 0.0)
        );
        assert_eq!(
            drawing
                .entity(copies[0])
                .unwrap()
                .bounds(cad_document::Tables::none())
                .min(),
            Point2::new(0.0, 50.0)
        );

        drawing.undo().unwrap();
        assert_eq!(drawing.len(), 3, "one undo removes both copies");
    }

    #[test]
    fn an_empty_edit_previews_and_commits_nothing() {
        let (mut drawing, _) = drawing();
        let edit = TransientEdit::default();
        let revision = drawing.revision();

        let mut overlay = Vec::new();
        edit.preview(by(1.0, 1.0), &mut overlay);
        edit.commit(&mut drawing, by(1.0, 1.0));

        assert!(overlay.is_empty() && edit.is_empty());
        assert_eq!(drawing.revision(), revision);
    }

    #[test]
    fn sources_that_no_longer_exist_are_skipped() {
        let (mut drawing, selection) = drawing();
        let edit = TransientEdit::of(&selection);
        let mut transaction = drawing.transaction();
        transaction.remove(edit.sources()[0]).unwrap();
        transaction.commit();

        edit.commit(&mut drawing, by(1.0, 0.0));
        assert_eq!(drawing.len(), 2);
    }
}
