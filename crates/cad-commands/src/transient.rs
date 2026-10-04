//! Transient edits of existing entities (DESIGN §17).
//!
//! While the pointer moves, an edit exists only as a description in the
//! overlay: the canonical document, its spatial index, and the committed
//! render scene are untouched. On commit, the edit becomes exactly one
//! transaction. Cancelling needs no cleanup, because nothing was changed.

use cad_core::{EntityId, Vec2};
use cad_document::{Drawing, Entity};

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

    /// The entities being edited, in ID order. Snapping must skip these while
    /// the edit is in progress (§18.2).
    pub fn sources(&self) -> &[EntityId] {
        &self.sources
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Shows the sources where a translation by `offset` would put them.
    pub fn preview_translated(&self, offset: Vec2, out: &mut Vec<OverlayItem>) {
        if !self.sources.is_empty() {
            out.push(OverlayItem::PreviewTranslated {
                ids: self.sources.clone(),
                offset,
            });
        }
    }

    /// Moves the sources by `offset` in one transaction. IDs are unchanged.
    pub fn commit_translated(&self, drawing: &mut Drawing, offset: Vec2) {
        let moved = self.translated(drawing, offset);
        let mut transaction = drawing.transaction();
        for (id, entity) in moved {
            transaction
                .replace(id, entity)
                .expect("sources are live and appear once");
        }
        transaction.commit();
    }

    /// Inserts copies of the sources, moved by `offset`, in one transaction.
    /// Returns the new IDs.
    pub fn commit_copies(&self, drawing: &mut Drawing, offset: Vec2) -> Vec<EntityId> {
        let copies = self.translated(drawing, offset);
        let mut transaction = drawing.transaction();
        let ids = copies
            .into_iter()
            .map(|(_, entity)| transaction.insert(entity))
            .collect();
        transaction.commit();
        ids
    }

    /// Each live source with its translated geometry.
    fn translated(&self, drawing: &Drawing, offset: Vec2) -> Vec<(EntityId, Entity)> {
        self.sources
            .iter()
            .filter_map(|&id| Some((id, drawing.entity(id)?.translated(offset))))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use cad_document::{CircleEntity, LineEntity};

    use super::*;

    fn drawing() -> (Drawing, Selection) {
        let mut drawing = Drawing::new();
        let mut transaction = drawing.transaction();
        let line = transaction.insert(LineEntity::new(
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
        ));
        let circle = transaction.insert(CircleEntity::new(Point2::new(5.0, 5.0), 2.0));
        transaction.insert(LineEntity::new(
            Point2::new(100.0, 100.0),
            Point2::new(101.0, 100.0),
        ));
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
            edit.preview_translated(Vec2::new(f64::from(step), 1.0), &mut overlay);
        }

        assert_eq!(
            overlay,
            vec![OverlayItem::PreviewTranslated {
                ids: edit.sources().to_vec(),
                offset: Vec2::new(999.0, 1.0)
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

        edit.commit_translated(&mut drawing, Vec2::new(100.0, 0.0));
        assert_eq!(
            drawing.entity(ids[0]).unwrap().bounds().min(),
            Point2::new(100.0, 0.0)
        );
        assert_eq!(
            drawing.entity(ids[1]).unwrap().bounds().min(),
            Point2::new(103.0, 3.0)
        );
        assert_eq!(drawing.len(), 3);

        drawing.undo().unwrap();
        assert_eq!(
            drawing.entity(ids[0]).unwrap().bounds().min(),
            Point2::new(0.0, 0.0)
        );
        assert!(!drawing.can_undo() || drawing.undo().is_some());
    }

    #[test]
    fn committing_copies_inserts_new_entities_and_leaves_the_sources() {
        let (mut drawing, selection) = drawing();
        let edit = TransientEdit::of(&selection);

        let copies = edit.commit_copies(&mut drawing, Vec2::new(0.0, 50.0));
        assert_eq!(copies.len(), 2);
        assert_eq!(drawing.len(), 5);
        assert!(
            copies.iter().all(|id| !edit.sources().contains(id)),
            "copies get fresh IDs"
        );
        assert_eq!(
            drawing.entity(edit.sources()[0]).unwrap().bounds().min(),
            Point2::new(0.0, 0.0)
        );
        assert_eq!(
            drawing.entity(copies[0]).unwrap().bounds().min(),
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
        edit.preview_translated(Vec2::new(1.0, 1.0), &mut overlay);
        edit.commit_translated(&mut drawing, Vec2::new(1.0, 1.0));

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

        edit.commit_translated(&mut drawing, Vec2::new(1.0, 0.0));
        assert_eq!(drawing.len(), 2);
    }
}
