//! The semantic description of transient geometry (ADR-0001 rule 5).
//!
//! Commands and the session say *what* should be shown; `cad-render` turns
//! that into packed geometry. Nothing here knows about vertices.

use cad_core::{EntityId, Point2};
use cad_document::Entity;

#[derive(Clone, Debug, PartialEq)]
pub enum OverlayItem {
    /// A selected entity to highlight.
    Highlight(EntityId),
    /// A rubber-band segment that is not itself an entity, such as the
    /// radius line while placing a circle.
    PreviewSegment { start: Point2, end: Point2 },
    /// The entity a command would create if the next input were the cursor.
    PreviewEntity(Entity),
    /// The cursor crosshair at a world position.
    Crosshair { at: Point2 },
}
