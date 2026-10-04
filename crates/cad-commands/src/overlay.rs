//! The semantic description of transient geometry (ADR-0001 rule 5).
//!
//! Commands and the session say *what* should be shown; `cad-render` turns
//! that into packed geometry. Nothing here knows about vertices.

use cad_core::{EntityId, Point2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OverlayItem {
    /// A selected entity to highlight.
    Highlight(EntityId),
    /// A rubber-band segment a command would create.
    PreviewSegment { start: Point2, end: Point2 },
    /// The cursor crosshair at a world position.
    Crosshair { at: Point2 },
}
