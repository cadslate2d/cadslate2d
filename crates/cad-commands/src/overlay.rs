//! The semantic description of transient geometry (ADR-0001 rule 5).
//!
//! Commands and the session say *what* should be shown; `cad-render` turns
//! that into packed geometry. Nothing here knows about vertices.

use cad_core::{EntityId, Point2};
use cad_document::Entity;
use cad_geometry::Transform;

use crate::constraint_display::Glyph;
use crate::{GripId, SnapMode};

#[derive(Clone, Debug, PartialEq)]
pub enum OverlayItem {
    /// The reference grid, with lines `spacing` drawing units apart. It
    /// depends on the view, so it lives here and not in the committed scene;
    /// it is drawn beneath the drawing.
    Grid { spacing: f64 },
    /// A selected entity to highlight.
    Highlight(EntityId),
    /// The entity under the cursor while a command asks for one object: what
    /// a click would pick.
    Hover(EntityId),
    /// Geometry an edit would remove if the hovered object were picked.
    PreviewRemoval(Entity),
    /// The grips of the selected entities.
    Grips { at: Vec<Point2> },
    /// The grip under the cursor, or the one being dragged (shown where it
    /// started).
    HotGrip { at: Point2 },
    /// The entity a grip edit would produce if the grip were dropped at
    /// `to` — or, for large entities, just the part of it that changes.
    PreviewGrip { id: GripId, to: Point2 },
    /// A rubber-band segment that is not itself an entity, such as the
    /// radius line while placing a circle.
    PreviewSegment { start: Point2, end: Point2 },
    /// The entity a command would create if the next input were the cursor.
    PreviewEntity(Entity),
    /// An object a proposal would add, or the state it would give one
    /// (#254): drawn in the proposal's own style.
    ProposalAdded(Entity),
    /// An object a proposal would remove, or the state it would change.
    ProposalRemoved(Entity),
    /// Existing entities shown where an edit would put them. The document is
    /// not touched until the edit is committed (DESIGN §17).
    PreviewTransformed {
        ids: Vec<EntityId>,
        transform: Transform,
    },
    /// The rectangle of a window or crossing selection in progress.
    SelectionBox {
        from: Point2,
        to: Point2,
        crossing: bool,
    },
    /// A fence or polygon selection in progress (#185): its points so
    /// far, the rubber band to `to`, and — for a polygon — the closing
    /// edge back to the first point.
    SelectionPath {
        points: Vec<Point2>,
        to: Point2,
        closed: bool,
        crossing: bool,
    },
    /// The direction polar tracking has locked onto: a ray from `from`
    /// through `through`, drawn to the edge of the view.
    TrackingLine { from: Point2, through: Point2 },
    /// A point object snap tracking runs from (#220), marked with a small
    /// cross.
    AcquiredPoint { at: Point2 },
    /// The object snap the next click would take, shown at its exact point.
    SnapMarker { mode: SnapMode, at: Point2 },
    /// The cursor crosshair at a world position.
    Crosshair { at: Point2 },
    /// The frame of the viewport the model is worked through (#193), in
    /// the model's coordinates: where the paper's window is.
    ViewportFrame { corners: [Point2; 4] },
    /// The constraint bar of an object (#269): its glyphs in a row beside
    /// `at`, laid out in pixels by `constraint_display::glyph_square`;
    /// `hot`, the one under the pointer.
    ConstraintBar {
        at: Point2,
        glyphs: Vec<Glyph>,
        hot: Option<usize>,
    },
    /// A dimensional constraint shown as a dimension, labelled with its
    /// name and value (#269).
    ConstraintDimension(Entity),
}
