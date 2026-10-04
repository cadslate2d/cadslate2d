//! The semantic description of transient geometry (ADR-0001 rule 5).
//!
//! Commands and the session say *what* should be shown; `cad-render` turns
//! that into packed geometry. Nothing here knows about vertices.

use cad_core::{EntityId, Point2, Vec2};
use cad_document::Entity;

use crate::SnapMode;

#[derive(Clone, Debug, PartialEq)]
pub enum OverlayItem {
    /// A selected entity to highlight.
    Highlight(EntityId),
    /// A rubber-band segment that is not itself an entity, such as the
    /// radius line while placing a circle.
    PreviewSegment { start: Point2, end: Point2 },
    /// The entity a command would create if the next input were the cursor.
    PreviewEntity(Entity),
    /// Existing entities shown where an edit would put them. The document is
    /// not touched until the edit is committed (DESIGN §17).
    PreviewTranslated { ids: Vec<EntityId>, offset: Vec2 },
    /// The rectangle of a window or crossing selection in progress.
    SelectionBox {
        from: Point2,
        to: Point2,
        crossing: bool,
    },
    /// The direction polar tracking has locked onto: a ray from `from`
    /// through `through`, drawn to the edge of the view.
    TrackingLine { from: Point2, through: Point2 },
    /// The object snap the next click would take, shown at its exact point.
    SnapMarker { mode: SnapMode, at: Point2 },
    /// The cursor crosshair at a world position.
    Crosshair { at: Point2 },
}
