//! Grips of the selected entities (DESIGN §16.2): which exist, and which one
//! is under the cursor.
//!
//! What each grip *does* is a pure function of the entity
//! (`Entity::grip_moved` in `cad-document`); the edit itself is a command
//! (`GripEditCommand`), so it shares previews, snapping, typed input, and
//! undo with every other command (§16.3).

use cad_core::{EntityId, PixelTolerance, Point2, ScreenPoint, View};
use cad_document::{Drawing, GripKind};

use crate::Selection;

/// With more entities than this selected, grips are not shown: they would
/// bury the drawing, and nobody grip-edits a hundred entities at once.
pub const GRIP_OBJECT_LIMIT: usize = 100;
/// Half the side of the square, in CSS pixels, within which a click takes a
/// grip. A picking tolerance, never a geometric one (§18.3).
pub const GRIP_APERTURE: PixelTolerance = PixelTolerance(6.0);

/// Identifies one grip: an entity and the grip's index among that entity's
/// grips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GripId {
    pub entity: EntityId,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    pub id: GripId,
    pub position: Point2,
    pub kind: GripKind,
}

/// The grips of every selected entity, in selection order.
pub fn grips_of(drawing: &Drawing, selection: &Selection) -> Vec<Grip> {
    if selection.len() > GRIP_OBJECT_LIMIT {
        return Vec::new();
    }
    selection
        .iter()
        .filter_map(|entity| Some((entity, drawing.entity(entity)?)))
        .flat_map(|(entity, shape)| {
            shape
                .grips()
                .into_iter()
                .enumerate()
                .map(move |(index, grip)| Grip {
                    id: GripId { entity, index },
                    position: grip.position,
                    kind: grip.kind,
                })
        })
        .collect()
}

/// The grip under `cursor`: the nearest one whose square the cursor is in.
/// Grips are squares on screen, so the distance is the larger of the two
/// axis distances.
pub fn grip_at(grips: &[Grip], view: &View, cursor: ScreenPoint) -> Option<Grip> {
    let reach = |grip: &Grip| {
        let at = view.world_to_screen(grip.position);
        (at.x - cursor.x).abs().max((at.y - cursor.y).abs())
    };
    grips
        .iter()
        .map(|grip| (reach(grip), grip))
        .filter(|(pixels, _)| *pixels <= GRIP_APERTURE.0)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, grip)| *grip)
}

#[cfg(test)]
mod tests {
    use cad_document::{CircleEntity, Entity, LineEntity};

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn scene() -> (Drawing, Vec<EntityId>) {
        let drawing = Drawing::from_entities([
            Entity::from(LineEntity::new(p(0.0, 0.0), p(100.0, 0.0))),
            Entity::from(CircleEntity::new(p(0.0, 100.0), 20.0)),
        ]);
        let mut ids: Vec<EntityId> = drawing.entities().map(|(id, _)| id).collect();
        ids.sort_unstable();
        (drawing, ids)
    }

    #[test]
    fn only_selected_entities_have_grips() {
        let (drawing, ids) = scene();
        let mut selection = Selection::default();
        assert!(grips_of(&drawing, &selection).is_empty());

        selection.add(ids[0]);
        let grips = grips_of(&drawing, &selection);
        assert_eq!(grips.len(), 3);
        assert!(grips.iter().all(|grip| grip.id.entity == ids[0]));
        assert_eq!(
            grips.iter().map(|grip| grip.id.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );

        selection.add(ids[1]);
        assert_eq!(grips_of(&drawing, &selection).len(), 3 + 5);
    }

    #[test]
    fn a_large_selection_shows_no_grips() {
        let lines = (0..=GRIP_OBJECT_LIMIT).map(|index| {
            let y = index as f64;
            Entity::from(LineEntity::new(p(0.0, y), p(10.0, y)))
        });
        let drawing = Drawing::from_entities(lines);
        let mut selection = Selection::default();
        let mut ids: Vec<EntityId> = drawing.entities().map(|(id, _)| id).collect();
        ids.sort_unstable();
        for id in &ids[..GRIP_OBJECT_LIMIT] {
            selection.add(*id);
        }
        assert_eq!(grips_of(&drawing, &selection).len(), 3 * GRIP_OBJECT_LIMIT);
        selection.add(ids[GRIP_OBJECT_LIMIT]);
        assert!(grips_of(&drawing, &selection).is_empty());
    }

    #[test]
    fn the_grip_under_the_cursor_is_the_nearest_within_its_square() {
        let (drawing, ids) = scene();
        let mut selection = Selection::default();
        selection.add(ids[0]);
        let grips = grips_of(&drawing, &selection);
        let view = View::new(800.0, 600.0);
        let near = |x: f64, y: f64| {
            let at = view.world_to_screen(p(x, y));
            grip_at(&grips, &view, at).map(|grip| grip.id.index)
        };

        assert_eq!(near(100.0, 0.0), Some(2));
        assert_eq!(near(95.0, 5.0), Some(2), "inside the square's corner");
        assert_eq!(near(93.0, 0.0), None, "seven pixels away");
        assert_eq!(near(52.0, -3.0), Some(1));
        assert_eq!(near(25.0, 0.0), None, "on the line, between grips");
    }

    #[test]
    fn the_grip_square_is_sized_in_pixels_at_any_zoom() {
        let (drawing, ids) = scene();
        let mut selection = Selection::default();
        selection.add(ids[0]);
        let grips = grips_of(&drawing, &selection);
        let mut view = View::new(800.0, 600.0);
        view.zoom_about(view.world_to_screen(p(100.0, 0.0)), 25.0);
        let end = view.world_to_screen(p(100.0, 0.0));
        let offset = |pixels: f64| ScreenPoint::new(end.x + pixels, end.y);
        assert!(grip_at(&grips, &view, offset(5.0)).is_some());
        assert!(grip_at(&grips, &view, offset(8.0)).is_none());
    }
}
