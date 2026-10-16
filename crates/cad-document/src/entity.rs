//! Semantic entity types (DESIGN §11).

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{
    Arc, Circle, Ellipse, Path, Primitive, Segment, SelectionShape, Spline, Transform, is_straight,
    polyline_primitives,
};

use crate::block::BlockId;
pub use crate::block::BlockReferenceEntity;
use crate::construction::{RayEntity, XLineEntity};
use crate::dimension::DimensionEntity;
use crate::hatch::HatchEntity;
use crate::leader::LeaderEntity;
use crate::mleader::{MLeaderContent, MLeaderEntity};
pub use crate::mtext::MTextEntity;
use crate::point::PointEntity;
use crate::tables::Tables;
pub use crate::text::TextEntity;
use crate::viewport::ViewportEntity;
use crate::wipeout::WipeoutEntity;

/// A straight line between two points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineEntity {
    pub start: Point2,
    pub end: Point2,
}

impl LineEntity {
    pub const fn new(start: Point2, end: Point2) -> Self {
        Self { start, end }
    }

    pub fn segment(&self) -> Segment {
        Segment::new(self.start, self.end)
    }
}

/// A full circle. It stays a circle in the document; only the renderer may
/// approximate it (DESIGN §11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleEntity {
    pub center: Point2,
    pub radius: f64,
}

impl CircleEntity {
    pub const fn new(center: Point2, radius: f64) -> Self {
        Self { center, radius }
    }

    pub fn circle(&self) -> Circle {
        Circle::new(self.center, self.radius)
    }
}

/// A circular arc, counter-clockwise from `start_angle` to `end_angle`
/// (radians from +X).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcEntity {
    pub center: Point2,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl ArcEntity {
    pub const fn new(center: Point2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self {
            center,
            radius,
            start_angle,
            end_angle,
        }
    }

    pub fn arc(&self) -> Arc {
        Arc::from_angles(self.center, self.radius, self.start_angle, self.end_angle)
    }
}

impl From<Arc> for ArcEntity {
    fn from(arc: Arc) -> Self {
        Self::new(arc.center, arc.radius, arc.start, arc.end())
    }
}

/// An ellipse, or an arc of one (#181): its centre, its major half-axis,
/// the minor axis as a ratio of it, and the parameters it runs between —
/// what DXF's `ELLIPSE` holds. Its curve is read as chords
/// (`cad_geometry::Ellipse`), so it snaps, picks, and intersects like a
/// polyline would, to a third of a thousandth of its major axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EllipseEntity {
    pub center: Point2,
    /// From the centre to one end of the long axis.
    pub major: Vec2,
    /// The minor half-axis over the major, in `(0, 1]`.
    pub ratio: f64,
    /// Start parameter, radians.
    pub start_parameter: f64,
    /// End parameter, radians; equal to the start for a whole ellipse.
    pub end_parameter: f64,
}

impl EllipseEntity {
    pub const fn new(center: Point2, major: Vec2, ratio: f64) -> Self {
        Self {
            center,
            major,
            ratio,
            start_parameter: 0.0,
            end_parameter: 0.0,
        }
    }

    pub const fn arc(center: Point2, major: Vec2, ratio: f64, start: f64, end: f64) -> Self {
        Self {
            center,
            major,
            ratio,
            start_parameter: start,
            end_parameter: end,
        }
    }

    pub fn ellipse(&self) -> Ellipse {
        Ellipse::from_parameters(
            self.center,
            self.major,
            self.ratio,
            self.start_parameter,
            self.end_parameter,
        )
    }

    pub fn is_full(&self) -> bool {
        self.ellipse().is_full()
    }
}

impl From<Ellipse> for EllipseEntity {
    fn from(ellipse: Ellipse) -> Self {
        if ellipse.is_full() {
            Self::new(ellipse.center, ellipse.major, ellipse.ratio)
        } else {
            Self::arc(
                ellipse.center,
                ellipse.major,
                ellipse.ratio,
                ellipse.start,
                ellipse.end(),
            )
        }
    }
}

/// An attribute definition (#183): a block's variable text — its tag, the
/// prompt a reference answers when placed, and its default — with the
/// placement of a text, which the value takes. Outside a block it is drawn
/// as its tag; inside one, a reference draws its value for the tag.
#[derive(Clone, Debug, PartialEq)]
pub struct AttributeDefinitionEntity {
    pub tag: String,
    pub prompt: String,
    /// The placement, style, and default: `text.content` is the default.
    pub text: TextEntity,
    /// Not drawn by a reference (DXF flag 1).
    pub invisible: bool,
}

impl AttributeDefinitionEntity {
    pub fn new(tag: impl Into<String>, prompt: impl Into<String>, text: TextEntity) -> Self {
        Self {
            tag: tag.into(),
            prompt: prompt.into(),
            text,
            invisible: false,
        }
    }

    /// How it is drawn on its own: its tag, where its value would be.
    pub fn as_text(&self) -> TextEntity {
        self.text.saying(self.tag.clone())
    }
}

/// A spline (#182): a B-spline by its degree, control points, knots, and
/// weights — what DXF's `SPLINE` holds — and the fit points it was drawn
/// through, when it was, which are then its grips. Its curve is read as
/// chords (`cad_geometry::Spline`), subdivided until flat to a quarter of a
/// thousandth of its extent.
#[derive(Clone, Debug, PartialEq)]
pub struct SplineEntity {
    pub degree: usize,
    pub control: Vec<Point2>,
    pub knots: Vec<f64>,
    /// One per control point, or empty when the spline is not rational.
    pub weights: Vec<f64>,
    /// The points the curve passes through, when it was defined by them.
    pub fit: Vec<Point2>,
}

impl SplineEntity {
    /// A cubic through `points`, in order; `None` when they do not make one.
    pub fn through(points: &[Point2]) -> Option<Self> {
        let spline = Spline::through(points)?;
        Some(Self {
            degree: spline.degree,
            control: spline.control,
            knots: spline.knots,
            weights: spline.weights,
            fit: points.to_vec(),
        })
    }

    /// A spline given by its control data alone.
    pub fn from_spline(spline: Spline) -> Self {
        Self {
            degree: spline.degree,
            control: spline.control,
            knots: spline.knots,
            weights: spline.weights,
            fit: Vec::new(),
        }
    }

    pub fn spline(&self) -> Spline {
        Spline {
            degree: self.degree,
            control: self.control.clone(),
            knots: self.knots.clone(),
            weights: self.weights.clone(),
        }
    }

    /// Whether the curve ends where it began.
    pub fn is_closed(&self) -> bool {
        let spline = self.spline();
        LinearTolerance::DEFAULT
            .around(&self.control)
            .coincident(spline.start_point(), spline.end_point())
    }

    /// The points that define the curve: the fit points when it was drawn
    /// through them, else the control points. Its grips.
    pub fn defining_points(&self) -> &[Point2] {
        if self.fit.is_empty() {
            &self.control
        } else {
            &self.fit
        }
    }

    /// The spline with its defining points replaced by `points`, one for
    /// one: re-fitted through them, or with the control points moved.
    pub fn redefined(&self, points: Vec<Point2>) -> Option<Self> {
        if points.len() != self.defining_points().len() {
            return None;
        }
        if self.fit.is_empty() {
            let moved = Self {
                control: points,
                ..self.clone()
            };
            moved.is_valid().then_some(moved)
        } else {
            Self::through(&points)
        }
    }

    /// Whether the data makes a spline, fit points included.
    pub fn is_valid(&self) -> bool {
        self.spline().is_valid()
            && self.fit.len() <= cad_geometry::MAX_POINTS
            && self.fit.iter().all(|point| point.is_finite())
    }
}

/// A chain of segments through at least two vertices, optionally closed back
/// to the first. A segment is straight unless the vertex it starts at has a
/// *bulge*, which makes it a circular arc (see `cad_geometry::polyline` for
/// the convention: `tan(included angle / 4)`, positive counter-clockwise).
///
/// Bulges are stored only as far as the last non-zero one, so two polylines
/// with the same shape always compare equal, and an all-straight polyline
/// carries no bulge data at all.
#[derive(Clone, Debug, PartialEq)]
pub struct PolylineEntity {
    pub vertices: Vec<Point2>,
    /// One per vertex, for the segment starting there; missing ones are zero.
    bulges: Vec<f64>,
    pub closed: bool,
    /// A constant width (#204): drawn as a filled band half this wide to
    /// either side of the path; zero for a plain polyline.
    width: f64,
}

impl PolylineEntity {
    /// A polyline of straight segments.
    pub fn new(vertices: Vec<Point2>, closed: bool) -> Self {
        Self {
            vertices,
            bulges: Vec::new(),
            closed,
            width: 0.0,
        }
    }

    /// This polyline with a constant width: a positive one drawn as a
    /// band, zero for none. Anything else is taken as none.
    #[must_use]
    pub fn with_width(self, width: f64) -> Self {
        Self {
            width: if width.is_finite() && width > 0.0 {
                width
            } else {
                0.0
            },
            ..self
        }
    }

    /// The constant width, or zero.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// This polyline closed, or opened: the closing segment is straight
    /// when closed, and an open polyline's last vertex starts no segment.
    #[must_use]
    pub fn with_closed(&self, closed: bool) -> Self {
        Self::with_bulges(self.vertices.clone(), self.bulges().collect(), closed)
            .with_width(self.width)
    }

    /// This polyline's geometry replaced by `path` — cut, grown, offset —
    /// with its width kept.
    #[must_use]
    pub fn with_path(&self, path: Path) -> Self {
        Self::with_bulges(path.vertices, path.bulges, path.closed).with_width(self.width)
    }

    /// The band the width fills, as triangles (ADR-0021): empty for a
    /// plain polyline.
    pub fn band(&self) -> Vec<[Point2; 3]> {
        self.path().band(self.width)
    }

    /// A polyline whose segment from vertex `i` has bulge `bulges[i]`.
    /// Missing bulges are zero; so are those of segments that do not exist
    /// (the last vertex of an open polyline) and any that are not numbers.
    pub fn with_bulges(vertices: Vec<Point2>, mut bulges: Vec<f64>, closed: bool) -> Self {
        let given = |index: usize| bulges.get(index).copied().unwrap_or(0.0);
        let segments = cad_geometry::polyline_spans(vertices.len(), closed, given).count();
        bulges.truncate(segments);
        for bulge in &mut bulges {
            if is_straight(*bulge) {
                *bulge = 0.0;
            }
        }
        let used = bulges
            .iter()
            .rposition(|bulge| bulge.abs() > 0.0)
            .map_or(0, |last| last + 1);
        bulges.truncate(used);
        Self {
            vertices,
            bulges,
            closed,
            width: 0.0,
        }
    }

    /// The bulge of the segment that starts at vertex `index`.
    pub fn bulge(&self, index: usize) -> f64 {
        self.bulges.get(index).copied().unwrap_or(0.0)
    }

    /// One bulge per vertex.
    pub fn bulges(&self) -> impl Iterator<Item = f64> + '_ {
        (0..self.vertices.len()).map(|index| self.bulge(index))
    }

    /// True when at least one segment is an arc.
    pub fn has_arcs(&self) -> bool {
        !self.bulges.is_empty()
    }

    /// The polyline as a path that can be measured along and cut.
    pub fn path(&self) -> Path {
        Path {
            vertices: self.vertices.clone(),
            bulges: self.bulges().collect(),
            closed: self.closed,
        }
    }

    /// The segments in order — straight segments and arcs — including the
    /// closing one when closed.
    pub fn primitives(&self) -> impl Iterator<Item = Primitive> + '_ {
        polyline_primitives(&self.vertices, |index| self.bulge(index), self.closed)
    }
}

impl From<Path> for PolylineEntity {
    fn from(path: Path) -> Self {
        Self::with_bulges(path.vertices, path.bulges, path.closed)
    }
}

/// Every kind of entity a drawing can hold.
///
/// Deliberately *not* `#[non_exhaustive]`: when a later milestone adds a
/// variant (polyline, text, …) the compiler must point at every match in the
/// workspace — rendering, picking, DXF — rather than letting a wildcard arm
/// silently ignore the new entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Line(LineEntity),
    Circle(CircleEntity),
    Arc(ArcEntity),
    /// An ellipse or an arc of one (#181), read as chords.
    Ellipse(EllipseEntity),
    /// A spline (#182), read as chords.
    Spline(SplineEntity),
    /// A block's variable text (#183): drawn as its tag on its own, as a
    /// reference's value inside a definition.
    AttributeDefinition(AttributeDefinitionEntity),
    Polyline(PolylineEntity),
    /// A single line of text (ADR-0016). Not a curve: it has no
    /// primitives, and is picked and selected by its box.
    Text(TextEntity),
    /// A paragraph of text, wrapped to a width; otherwise like a text.
    MText(MTextEntity),
    /// A placed block (ADR-0017). Its geometry is its definition's, read
    /// from the block table whenever it is asked for.
    BlockRef(BlockReferenceEntity),
    /// A dimension (ADR-0020). Its lines, arrowheads, and text are laid
    /// out from its points by its style, read from the dimension style
    /// table whenever it is asked for.
    Dimension(DimensionEntity),
    /// A filled region (ADR-0021): boundary loops and a fill, cut into
    /// triangles whenever it is asked for.
    Hatch(HatchEntity),
    /// A window on a layout's paper onto model space (#192, ADR-0030):
    /// drawn as its frame; its contents are the model's scene, drawn
    /// again under its own transform.
    Viewport(ViewportEntity),
    /// A place, drawn as the drawing's point style (#198).
    Point(PointEntity),
    /// A line to a note, with an arrowhead, laid out by a dimension style (#205).
    Leader(LeaderEntity),
    /// A construction line endless both ways (#206; ADR-0031).
    XLine(XLineEntity),
    /// A construction line endless one way (#206; ADR-0031).
    Ray(RayEntity),
    /// A polygon that masks what is beneath it (#226; ADR-0032).
    Wipeout(WipeoutEntity),
    /// Leaders from several places to one note, laid out by a multileader
    /// style (#240; ADR-0033).
    MLeader(MLeaderEntity),
}

impl Entity {
    /// World-space bounding box, used by the spatial index. A block
    /// reference's comes from its definition, a dimension's from its
    /// style, in `tables`.
    pub fn bounds(&self, tables: Tables<'_>) -> Aabb {
        match self {
            Entity::Line(line) => line.segment().bounds(),
            Entity::Circle(circle) => circle.circle().bounds(),
            Entity::Arc(arc) => arc.arc().bounds(),
            Entity::Ellipse(ellipse) => ellipse.ellipse().bounds(),
            Entity::Spline(spline) => spline.spline().bounds(),
            // Arc segments can reach beyond the vertices. Every polyline in a
            // drawing has segments; one without bounds nothing useful.
            Entity::Polyline(polyline) => polyline
                .primitives()
                .map(|primitive| primitive.bounds())
                .reduce(|a, b| a.union(&b))
                .unwrap_or_else(|| Aabb::from_points(Point2::ORIGIN, Point2::ORIGIN))
                .expanded(polyline.width() / 2.0),
            Entity::Text(text) => text.bounds(),
            Entity::MText(text) => text.bounds(),
            Entity::AttributeDefinition(definition) => definition.as_text().bounds(),
            Entity::BlockRef(reference) => tables.bounds_of(reference),
            Entity::Dimension(dimension) => {
                dimension.layout(tables.dim_style(dimension.style)).bounds()
            }
            Entity::Hatch(hatch) => hatch.bounds(),
            Entity::Viewport(viewport) => viewport.frame(),
            Entity::Point(point) => point.bounds(tables.point_style),
            Entity::Leader(leader) => leader.layout(tables.dim_style(leader.style)).bounds(),
            // What every geometric question reads: its reach (ADR-0031).
            // It is kept out of the index's tree and the extents.
            Entity::XLine(line) => line.segment().bounds(),
            Entity::Ray(ray) => ray.segment().bounds(),
            Entity::Wipeout(wipeout) => wipeout.bounds(),
            Entity::MLeader(mleader) => mleader
                .layout(tables.mleader_style(mleader.style), tables)
                .bounds(),
        }
    }

    /// The block the entity places: a reference's, or a multileader's
    /// block note's (#241).
    pub fn placed_block(&self) -> Option<BlockId> {
        match self {
            Entity::BlockRef(reference) => Some(reference.block),
            Entity::MLeader(MLeaderEntity {
                content: MLeaderContent::Block { block, .. },
                ..
            }) => Some(*block),
            _ => None,
        }
    }

    /// Whether the entity is drawn as an area — a solid hatch, a wipeout,
    /// a wide polyline — rather than as strokes: within a tier of the
    /// drawing order, an area lies beneath every stroke (ADR-0032).
    pub fn is_fill(&self) -> bool {
        match self {
            Entity::Hatch(hatch) => matches!(hatch.style, crate::HatchStyle::Solid),
            Entity::Wipeout(_) => true,
            Entity::Polyline(polyline) => polyline.width() > 0.0,
            _ => false,
        }
    }

    /// Exact distance from `point` to the entity's geometry, used for picking.
    pub fn distance_to(&self, point: Point2, tables: Tables<'_>) -> f64 {
        match self {
            Entity::Line(line) => line.segment().distance_to_point(point),
            Entity::Circle(circle) => circle.circle().distance_to_point(point),
            Entity::Arc(arc) => arc.arc().distance_to_point(point),
            Entity::Ellipse(ellipse) => ellipse.ellipse().distance_to_point(point),
            Entity::Spline(spline) => spline.spline().distance_to_point(point),
            // A wide polyline is picked anywhere on its band (#204).
            Entity::Polyline(polyline) => polyline
                .primitives()
                .map(|primitive| {
                    (primitive.distance_to_point(point) - polyline.width() / 2.0).max(0.0)
                })
                .fold(f64::INFINITY, f64::min),
            Entity::Text(text) => text.distance_to(point),
            Entity::MText(text) => text.distance_to(point),
            Entity::AttributeDefinition(definition) => definition.as_text().distance_to(point),
            Entity::BlockRef(reference) => tables
                .blocks
                .flatten(reference, crate::Properties::default())
                .iter()
                .map(|(entity, _)| entity.distance_to(point, tables))
                .fold(f64::INFINITY, f64::min),
            // A dimension is picked by its lines, arrowheads, and text box.
            Entity::Dimension(dimension) => dimension
                .layout(tables.dim_style(dimension.style))
                .distance_to(point),
            // A hatch by its fill and its boundary.
            Entity::Hatch(hatch) => hatch.distance_to(point),
            // A viewport by its frame: what shows through it is the model's.
            Entity::Viewport(viewport) => viewport.distance_to(point),
            Entity::Point(place) => place.distance_to(point),
            // A leader by its lines, arrowhead, and note's box.
            Entity::Leader(leader) => leader
                .layout(tables.dim_style(leader.style))
                .distance_to(point),
            Entity::XLine(line) => line.distance_to(point),
            Entity::Ray(ray) => ray.distance_to(point),
            // A wipeout by its frame, as a closed polyline is.
            Entity::Wipeout(wipeout) => wipeout.distance_to(point),
            // By its lines, arrowheads, and note, as a leader is.
            Entity::MLeader(mleader) => mleader
                .layout(tables.mleader_style(mleader.style), tables)
                .distance_to(point),
        }
    }

    /// The entity as lines, circles, and arcs: the common currency of
    /// intersection, snapping, and trimming. A block reference's are its
    /// definition's, placed.
    pub fn primitives(&self, tables: Tables<'_>) -> Vec<Primitive> {
        match self {
            Entity::Line(line) => vec![Primitive::Segment(line.segment())],
            Entity::Circle(circle) => vec![Primitive::Circle(circle.circle())],
            Entity::Arc(arc) => vec![Primitive::Arc(arc.arc())],
            // An ellipse is its chords (#181): a third of a thousandth of
            // its major axis from the curve.
            Entity::Ellipse(ellipse) => ellipse
                .ellipse()
                .segments()
                .into_iter()
                .map(Primitive::Segment)
                .collect(),
            // A spline likewise (#182), flat to a quarter of a thousandth of its extent.
            Entity::Spline(spline) => spline
                .spline()
                .segments()
                .into_iter()
                .map(Primitive::Segment)
                .collect(),
            Entity::Polyline(polyline) => polyline.primitives().collect(),
            // Nothing intersects, trims against, or snaps to the strokes
            // of a text, nor to the lines of a dimension: an annotation is
            // not geometry of the drawing.
            Entity::Text(_)
            | Entity::MText(_)
            | Entity::AttributeDefinition(_)
            | Entity::Dimension(_)
            | Entity::Hatch(_) => Vec::new(),
            // A viewport's frame is a window, not geometry of the drawing:
            // nothing trims against it. Its corners snap (see `snap.rs`).
            Entity::Viewport(_) => Vec::new(),
            // A point is a place: it snaps as a node and cuts nothing.
            Entity::Point(_) => Vec::new(),
            // A leader is annotation, like a dimension (ADR-0020), and so
            // is a multileader (ADR-0033).
            Entity::Leader(_) | Entity::MLeader(_) => Vec::new(),
            // A construction line is its reach (ADR-0031): it cuts, is
            // cut, and snaps as a segment that long.
            Entity::XLine(line) => vec![Primitive::Segment(line.segment())],
            Entity::Ray(ray) => vec![Primitive::Segment(ray.segment())],
            // A wipeout's frame is a polygon: it snaps and cuts as one.
            Entity::Wipeout(wipeout) => wipeout.primitives(),
            Entity::BlockRef(reference) => tables
                .blocks
                .flatten(reference, crate::Properties::default())
                .iter()
                .flat_map(|(entity, _)| entity.primitives(tables))
                .collect(),
        }
    }

    /// Every point where this entity's geometry meets `other`'s.
    pub fn intersections(&self, other: &Entity, tables: Tables<'_>) -> Vec<Point2> {
        // A construction line meets things where they are: read it within
        // the other's bounds, not along its million-unit reach, so the
        // crossing is computed at their scale (ADR-0031).
        match (self.construction(), other.construction()) {
            (Some(mine), Some(theirs)) => {
                return crate::construction::crossing(mine, theirs)
                    .into_iter()
                    .collect();
            }
            (Some(_), None) => {
                let near = other.bounds(tables).expanded(1.0);
                return self
                    .within(&near)
                    .map_or_else(Vec::new, |local| local.intersections(other, tables));
            }
            (None, Some(_)) => {
                let near = self.bounds(tables).expanded(1.0);
                return other
                    .within(&near)
                    .map_or_else(Vec::new, |local| self.intersections(&local, tables));
            }
            (None, None) => {}
        }
        let theirs = other.primitives(tables);
        self.primitives(tables)
            .iter()
            .flat_map(|mine| theirs.iter().map(move |their| mine.intersect(their)))
            .flat_map(|hits| hits.as_slice().to_vec())
            .collect()
    }

    /// The geometry of an entity that is not a block reference or a
    /// dimension, for the editing operations, which never take one: a
    /// reference has none of its own, and a dimension's is not a curve.
    pub(crate) fn own_primitives(&self) -> Vec<Primitive> {
        self.primitives(Tables::none())
    }

    pub(crate) fn own_distance_to(&self, point: Point2) -> f64 {
        self.distance_to(point, Tables::none())
    }

    pub(crate) fn own_bounds(&self) -> Aabb {
        self.bounds(Tables::none())
    }

    /// True when the whole entity lies within `bounds` (window selection).
    pub fn is_inside(&self, bounds: &Aabb, tables: Tables<'_>) -> bool {
        bounds.contains(&self.bounds(tables))
    }

    /// True when any part of the entity's geometry lies within `bounds`
    /// (crossing selection). Interiors do not count.
    pub fn crosses(&self, bounds: &Aabb, tables: Tables<'_>) -> bool {
        self.selected_by(&SelectionShape::Crossing(*bounds), tables)
    }

    /// Whether a selection shape — a box, a polygon, a fence (#185) —
    /// takes this entity. The shape judges by the entity's edges and by
    /// what it encloses, so a fence through a polyline's arc segment takes
    /// the polyline, and a polygon drawn inside a hatch takes the hatch.
    pub fn selected_by(&self, shape: &SelectionShape, tables: Tables<'_>) -> bool {
        shape.takes(&self.selection_edges(tables), |point| {
            self.encloses(point, tables)
        })
    }

    /// The curves a selection shape is tested against: the geometry of a
    /// curve, the frame of a text, the lines, arcs, and text frame of a
    /// dimension, the boundary of a hatch, and a block's, placed.
    fn selection_edges(&self, tables: Tables<'_>) -> Vec<Primitive> {
        match self {
            Entity::Line(_)
            | Entity::Circle(_)
            | Entity::Arc(_)
            | Entity::Ellipse(_)
            | Entity::Spline(_)
            | Entity::Polyline(_)
            | Entity::Wipeout(_) => self.primitives(tables),
            Entity::Text(text) => frame_edges(&text.frame()),
            Entity::MText(text) => frame_edges(&text.frame()),
            Entity::AttributeDefinition(definition) => frame_edges(&definition.as_text().frame()),
            Entity::BlockRef(reference) => tables
                .blocks
                .flatten(reference, crate::Properties::default())
                .iter()
                .flat_map(|(entity, _)| entity.selection_edges(tables))
                .collect(),
            Entity::Dimension(dimension) => {
                let layout = dimension.layout(tables.dim_style(dimension.style));
                layout
                    .segments()
                    .map(Primitive::Segment)
                    .chain(layout.arcs.iter().copied().map(Primitive::Arc))
                    .chain(frame_edges(&layout.text.frame()))
                    .collect()
            }
            Entity::Hatch(hatch) => hatch.boundary(),
            Entity::Viewport(viewport) => viewport
                .edges()
                .into_iter()
                .map(Primitive::Segment)
                .collect(),
            // A point is in a window when its place is; a crossing window
            // takes it the same way: an edge of no length at its place.
            Entity::Point(point) => vec![Primitive::Segment(Segment::new(
                point.position,
                point.position,
            ))],
            Entity::Leader(leader) => {
                let layout = leader.layout(tables.dim_style(leader.style));
                layout
                    .segments()
                    .map(Primitive::Segment)
                    .chain(
                        layout
                            .text
                            .iter()
                            .flat_map(|text| frame_edges(&text.frame())),
                    )
                    .collect()
            }
            Entity::XLine(line) => vec![Primitive::Segment(line.segment())],
            Entity::Ray(ray) => vec![Primitive::Segment(ray.segment())],
            Entity::MLeader(mleader) => {
                let layout = mleader.layout(tables.mleader_style(mleader.style), tables);
                layout
                    .segments()
                    .chain(layout.block_edges())
                    .map(Primitive::Segment)
                    .chain(
                        layout
                            .text
                            .iter()
                            .flat_map(|text| frame_edges(&text.frame())),
                    )
                    .collect()
            }
        }
    }

    /// Whether `point` lies in the entity's own area: a text's box, a
    /// hatch's fill, a dimension's text box, or any of a block's. A curve
    /// has no area.
    fn encloses(&self, point: Point2, tables: Tables<'_>) -> bool {
        match self {
            Entity::Line(_)
            | Entity::Circle(_)
            | Entity::Arc(_)
            | Entity::Ellipse(_)
            | Entity::Spline(_)
            | Entity::Polyline(_) => false,
            Entity::Text(text) => text.frame().contains(point),
            Entity::MText(text) => text.frame().contains(point),
            Entity::AttributeDefinition(definition) => definition.as_text().frame().contains(point),
            Entity::BlockRef(reference) => tables
                .blocks
                .flatten(reference, crate::Properties::default())
                .iter()
                .any(|(entity, _)| entity.encloses(point, tables)),
            Entity::Dimension(dimension) => dimension
                .layout(tables.dim_style(dimension.style))
                .text
                .frame()
                .contains(point),
            Entity::Hatch(hatch) => hatch.contains(point),
            Entity::Viewport(viewport) => viewport.contains(point),
            Entity::Point(_) => false,
            Entity::Leader(leader) => leader
                .layout(tables.dim_style(leader.style))
                .text
                .is_some_and(|text| text.frame().contains(point)),
            Entity::XLine(_) | Entity::Ray(_) => false,
            Entity::Wipeout(wipeout) => wipeout.contains(point),
            Entity::MLeader(mleader) => mleader
                .layout(tables.mleader_style(mleader.style), tables)
                .note_contains(point),
        }
    }

    /// A copy of the entity moved by `offset`. Exact: coordinates are added
    /// to, and nothing else changes.
    pub fn translated(&self, offset: Vec2) -> Self {
        self.transformed(&Transform::translation(offset))
    }

    /// A copy of the entity moved, rotated, uniformly scaled, or mirrored.
    /// Every kind of entity stays its kind (DESIGN §22).
    pub fn transformed(&self, transform: &Transform) -> Self {
        match self {
            Entity::Line(line) => {
                LineEntity::new(transform.apply(line.start), transform.apply(line.end)).into()
            }
            Entity::Circle(circle) => CircleEntity::new(
                transform.apply(circle.center),
                circle.radius * transform.scale_factor(),
            )
            .into(),
            Entity::Ellipse(ellipse) => {
                EllipseEntity::from(ellipse.ellipse().transformed(transform)).into()
            }
            Entity::Spline(spline) => SplineEntity {
                control: spline
                    .control
                    .iter()
                    .map(|point| transform.apply(*point))
                    .collect(),
                fit: spline
                    .fit
                    .iter()
                    .map(|point| transform.apply(*point))
                    .collect(),
                ..spline.clone()
            }
            .into(),
            Entity::Arc(arc) => {
                // Arcs run counter-clockwise from start to end. A mirror
                // reverses direction, so the two ends trade places.
                let (start, end) = if transform.reverses_orientation() {
                    (arc.end_angle, arc.start_angle)
                } else {
                    (arc.start_angle, arc.end_angle)
                };
                ArcEntity::new(
                    transform.apply(arc.center),
                    arc.radius * transform.scale_factor(),
                    transform.apply_angle(start),
                    transform.apply_angle(end),
                )
                .into()
            }
            Entity::Polyline(polyline) => {
                // Vertices keep their order, so a mirror turns every
                // counter-clockwise arc segment into a clockwise one.
                let flip = if transform.reverses_orientation() {
                    -1.0
                } else {
                    1.0
                };
                PolylineEntity::with_bulges(
                    polyline
                        .vertices
                        .iter()
                        .map(|vertex| transform.apply(*vertex))
                        .collect(),
                    polyline.bulges().map(|bulge| bulge * flip).collect(),
                    polyline.closed,
                )
                .with_width(polyline.width() * transform.scale_factor())
                .into()
            }
            Entity::Text(text) => text.transformed(transform).into(),
            Entity::MText(text) => text.transformed(transform).into(),
            Entity::AttributeDefinition(definition) => AttributeDefinitionEntity {
                text: definition.text.transformed(transform),
                ..definition.clone()
            }
            .into(),
            Entity::BlockRef(reference) => reference.transformed(transform).into(),
            Entity::Dimension(dimension) => dimension.transformed(transform).into(),
            Entity::Hatch(hatch) => hatch.transformed(transform).into(),
            Entity::Viewport(viewport) => viewport.transformed(transform).into(),
            Entity::Point(point) => point.transformed(transform).into(),
            Entity::Leader(leader) => leader.transformed(transform).into(),
            Entity::Wipeout(wipeout) => wipeout.transformed(transform).into(),
            Entity::MLeader(mleader) => mleader.transformed(transform).into(),
            Entity::XLine(line) => line.transformed(transform).into(),
            Entity::Ray(ray) => ray.transformed(transform).into(),
        }
    }

    pub fn as_line(&self) -> Option<&LineEntity> {
        match self {
            Entity::Line(line) => Some(line),
            _ => None,
        }
    }
}

impl From<LineEntity> for Entity {
    fn from(line: LineEntity) -> Self {
        Entity::Line(line)
    }
}

impl From<CircleEntity> for Entity {
    fn from(circle: CircleEntity) -> Self {
        Entity::Circle(circle)
    }
}

impl From<ViewportEntity> for Entity {
    fn from(viewport: ViewportEntity) -> Self {
        Entity::Viewport(viewport)
    }
}

impl From<PointEntity> for Entity {
    fn from(point: PointEntity) -> Self {
        Entity::Point(point)
    }
}

impl From<LeaderEntity> for Entity {
    fn from(leader: LeaderEntity) -> Self {
        Entity::Leader(leader)
    }
}

impl From<XLineEntity> for Entity {
    fn from(line: XLineEntity) -> Self {
        Entity::XLine(line)
    }
}

impl From<RayEntity> for Entity {
    fn from(ray: RayEntity) -> Self {
        Entity::Ray(ray)
    }
}

impl Entity {
    /// A construction line's base point, unit direction, and where along
    /// it it starts: −∞ for an xline, 0 for a ray. `None` for the rest.
    pub fn construction(&self) -> Option<(Point2, Vec2, f64)> {
        match self {
            Entity::XLine(line) => Some((line.base, line.direction, f64::NEG_INFINITY)),
            Entity::Ray(ray) => Some((ray.start, ray.direction, 0.0)),
            _ => None,
        }
    }

    /// A construction line as a line within `bounds` — what of it a
    /// question about that region needs — or `None` when it misses them.
    /// Any other entity as it is.
    pub fn within(&self, bounds: &Aabb) -> Option<Entity> {
        match self.construction() {
            Some((base, direction, from)) => {
                crate::construction::clip_to(base, direction, from, f64::INFINITY, bounds)
                    .map(|piece| LineEntity::new(piece.start, piece.end).into())
            }
            None => Some(self.clone()),
        }
    }

    /// Whether the entity runs for ever (ADR-0031): an xline or a ray,
    /// kept out of the spatial index's tree and the extents, and drawn
    /// clipped to the view.
    pub fn is_unbounded(&self) -> bool {
        matches!(self, Entity::XLine(_) | Entity::Ray(_))
    }
}

impl From<MLeaderEntity> for Entity {
    fn from(mleader: MLeaderEntity) -> Self {
        Entity::MLeader(mleader)
    }
}

impl From<WipeoutEntity> for Entity {
    fn from(wipeout: WipeoutEntity) -> Self {
        Entity::Wipeout(wipeout)
    }
}

impl From<PolylineEntity> for Entity {
    fn from(polyline: PolylineEntity) -> Self {
        Entity::Polyline(polyline)
    }
}

impl From<AttributeDefinitionEntity> for Entity {
    fn from(definition: AttributeDefinitionEntity) -> Self {
        Entity::AttributeDefinition(definition)
    }
}

impl From<SplineEntity> for Entity {
    fn from(spline: SplineEntity) -> Self {
        Entity::Spline(spline)
    }
}

impl From<EllipseEntity> for Entity {
    fn from(ellipse: EllipseEntity) -> Self {
        Entity::Ellipse(ellipse)
    }
}

impl From<ArcEntity> for Entity {
    fn from(arc: ArcEntity) -> Self {
        Entity::Arc(arc)
    }
}

impl From<TextEntity> for Entity {
    fn from(text: TextEntity) -> Self {
        Entity::Text(text)
    }
}

impl From<MTextEntity> for Entity {
    fn from(text: MTextEntity) -> Self {
        Entity::MText(text)
    }
}

impl From<BlockReferenceEntity> for Entity {
    fn from(reference: BlockReferenceEntity) -> Self {
        Entity::BlockRef(reference)
    }
}

impl From<DimensionEntity> for Entity {
    fn from(dimension: DimensionEntity) -> Self {
        Entity::Dimension(dimension)
    }
}

impl From<HatchEntity> for Entity {
    fn from(hatch: HatchEntity) -> Self {
        Entity::Hatch(hatch)
    }
}

/// An immutable entity payload shared between the store and undo history, so
/// history never deep-copies large entities (DESIGN §19.4).
pub type SharedEntity = std::sync::Arc<Entity>;

/// A text frame's four edges, as primitives.
fn frame_edges(frame: &crate::text::TextFrame) -> Vec<Primitive> {
    frame.edges().into_iter().map(Primitive::Segment).collect()
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::*;

    fn line() -> Entity {
        LineEntity::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)).into()
    }

    #[test]
    fn line_bounds_cover_its_endpoints() {
        let bounds = line().bounds(Tables::none());
        assert_eq!(bounds.min(), Point2::new(0.0, 0.0));
        assert_eq!(bounds.max(), Point2::new(10.0, 0.0));
    }

    #[test]
    fn line_distance_is_point_to_segment_distance() {
        assert_eq!(
            line().distance_to(Point2::new(5.0, 2.0), Tables::none()),
            2.0
        );
    }

    #[test]
    fn a_circle_is_bounded_and_picked_by_its_curve() {
        let circle: Entity = CircleEntity::new(Point2::new(10.0, 10.0), 4.0).into();
        assert_eq!(
            circle.bounds(Tables::none()),
            Aabb::from_points(Point2::new(6.0, 6.0), Point2::new(14.0, 14.0))
        );
        assert_eq!(
            circle.distance_to(Point2::new(10.0, 10.0), Tables::none()),
            4.0,
            "the interior is not the circle"
        );
        assert_eq!(
            circle.distance_to(Point2::new(15.0, 10.0), Tables::none()),
            1.0
        );
    }

    #[test]
    fn an_arc_runs_counter_clockwise_between_its_angles() {
        let arc = ArcEntity::new(Point2::ORIGIN, 10.0, 0.0, FRAC_PI_2);
        let entity: Entity = arc.into();
        assert!(entity.distance_to(Point2::new(0.0, 12.0), Tables::none()) - 2.0 < 1e-12);
        // Opposite the arc: far from it, although on its circle.
        assert!(entity.distance_to(Point2::new(-10.0, 0.0), Tables::none()) > 14.0);
        assert!((arc.arc().sweep - FRAC_PI_2).abs() < 1e-12);

        // The geometry type and the entity agree in both directions.
        let back: ArcEntity = Arc::new(Point2::ORIGIN, 10.0, PI, FRAC_PI_2).into();
        assert!((back.start_angle - PI).abs() < 1e-12 && (back.end_angle - 1.5 * PI).abs() < 1e-12);
    }

    #[test]
    fn a_polyline_is_bounded_by_its_vertices_and_picked_on_its_segments() {
        let rectangle: Entity = PolylineEntity::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 5.0),
                Point2::new(0.0, 5.0),
            ],
            true,
        )
        .into();
        assert_eq!(
            rectangle.bounds(Tables::none()),
            Aabb::from_points(Point2::new(0.0, 0.0), Point2::new(10.0, 5.0))
        );
        assert_eq!(
            rectangle.distance_to(Point2::new(5.0, 2.5), Tables::none()),
            2.5,
            "the interior is not the polyline"
        );
        assert_eq!(
            rectangle.distance_to(Point2::new(-1.0, 2.5), Tables::none()),
            1.0,
            "the closing segment counts"
        );
    }

    fn one_of_each() -> Vec<Entity> {
        vec![
            LineEntity::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)).into(),
            CircleEntity::new(Point2::new(5.0, 5.0), 3.0).into(),
            ArcEntity::new(Point2::new(5.0, 5.0), 4.0, 0.0, std::f64::consts::PI).into(),
            PolylineEntity::new(
                vec![
                    Point2::new(0.0, 0.0),
                    Point2::new(4.0, 0.0),
                    Point2::new(4.0, 4.0),
                ],
                true,
            )
            .into(),
            PointEntity::new(Point2::new(7.0, 2.0)).into(),
            WipeoutEntity::new(vec![
                Point2::new(1.0, 1.0),
                Point2::new(6.0, 1.0),
                Point2::new(3.0, 4.0),
            ])
            .unwrap()
            .into(),
        ]
    }

    /// Sample points along an entity: enough to tell whether two entities
    /// trace the same curve.
    fn samples(entity: &Entity) -> Vec<Point2> {
        match entity {
            Entity::Line(line) => vec![line.start, line.end],
            Entity::Circle(circle) => vec![circle.center],
            Entity::Arc(arc) => {
                let arc = arc.arc();
                vec![arc.start_point(), arc.mid_point(), arc.end_point()]
            }
            Entity::Ellipse(ellipse) => {
                let ellipse = ellipse.ellipse();
                vec![
                    ellipse.start_point(),
                    ellipse.point_at(ellipse.start + ellipse.sweep / 2.0),
                    ellipse.end_point(),
                ]
            }
            Entity::Spline(spline) => spline.control.clone(),
            Entity::Polyline(polyline) => polyline.vertices.clone(),
            Entity::Text(text) => text.frame().corners().to_vec(),
            Entity::MText(text) => text.frame().corners().to_vec(),
            Entity::AttributeDefinition(definition) => {
                definition.as_text().frame().corners().to_vec()
            }
            Entity::BlockRef(reference) => vec![reference.insertion],
            Entity::Dimension(dimension) => dimension.points(),
            Entity::Hatch(hatch) => hatch.vertices(),
            Entity::Viewport(viewport) => viewport.corners().to_vec(),
            Entity::Point(point) => vec![point.position],
            Entity::Leader(leader) => leader.vertices.clone(),
            Entity::XLine(line) => vec![line.base, line.base + line.direction * 10.0],
            Entity::Ray(ray) => vec![ray.start, ray.start + ray.direction * 10.0],
            Entity::Wipeout(wipeout) => wipeout.vertices.clone(),
            Entity::MLeader(mleader) => mleader.points(),
        }
    }

    fn same_curve(a: &Entity, b: &Entity) -> bool {
        std::mem::discriminant(a) == std::mem::discriminant(b)
            && samples(a)
                .iter()
                .zip(samples(b))
                .all(|(a, b)| a.distance_to(b) < 1e-9)
            && (a
                .bounds(Tables::none())
                .max()
                .distance_to(b.bounds(Tables::none()).max())
                < 1e-9)
    }

    #[test]
    fn every_kind_keeps_its_kind_under_every_transform() {
        let about = Point2::new(3.0, -2.0);
        let transforms = [
            Transform::translation(Vec2::new(7.0, 1.0)),
            Transform::rotation(about, 37.0),
            Transform::scaling(about, 2.5).unwrap(),
            Transform::mirror(about, Point2::new(8.0, 5.0)).unwrap(),
        ];
        for entity in one_of_each() {
            for transform in &transforms {
                let image = entity.transformed(transform);
                assert_eq!(
                    std::mem::discriminant(&image),
                    std::mem::discriminant(&entity)
                );
                // Every sample point of the original maps onto the image.
                for point in samples(&entity) {
                    if matches!(entity, Entity::Circle(_)) {
                        continue; // the centre is not on the curve
                    }
                    let distance = image.distance_to(transform.apply(point), Tables::none());
                    assert!(
                        distance < 1e-9,
                        "{entity:?} under {transform:?}: {distance}"
                    );
                }
            }
        }
    }

    /// #93: rotating by 360° reproduces the original geometry within tolerance.
    #[test]
    fn a_full_turn_reproduces_every_entity() {
        let about = Point2::new(-4.0, 9.0);
        for entity in one_of_each() {
            let full = entity.transformed(&Transform::rotation(about, 360.0));
            assert!(same_curve(&full, &entity), "{entity:?} became {full:?}");
            let mut stepped = entity.clone();
            for _ in 0..8 {
                stepped = stepped.transformed(&Transform::rotation(about, 45.0));
            }
            assert!(
                same_curve(&stepped, &entity),
                "{entity:?} became {stepped:?}"
            );
        }
    }

    /// #95: mirroring twice across the same line reproduces the original.
    #[test]
    fn mirroring_twice_reproduces_every_entity() {
        let mirror = Transform::mirror(Point2::new(1.0, 2.0), Point2::new(6.0, -3.0)).unwrap();
        for entity in one_of_each() {
            let twice = entity.transformed(&mirror).transformed(&mirror);
            assert!(same_curve(&twice, &entity), "{entity:?} became {twice:?}");
        }
    }

    #[test]
    fn a_rotated_arc_turns_both_angles() {
        let arc: Entity = ArcEntity::new(Point2::new(10.0, 0.0), 5.0, 0.0, FRAC_PI_2).into();
        let Entity::Arc(image) = arc.transformed(&Transform::rotation(Point2::ORIGIN, 90.0)) else {
            panic!("an arc stays an arc");
        };
        assert_eq!(image.center, Point2::new(0.0, 10.0));
        assert_eq!(image.radius, 5.0);
        assert!((image.start_angle - FRAC_PI_2).abs() < 1e-15);
        assert!((image.end_angle - PI).abs() < 1e-15);
    }

    /// #95: a mirrored arc keeps a valid counter-clockwise representation.
    #[test]
    fn a_mirrored_arc_swaps_its_ends_and_stays_counter_clockwise() {
        // A quarter arc from (10,0) up to (0,10), mirrored across the Y axis.
        let arc: Entity = ArcEntity::new(Point2::ORIGIN, 10.0, 0.0, FRAC_PI_2).into();
        let mirror = Transform::mirror(Point2::ORIGIN, Point2::new(0.0, 1.0)).unwrap();
        let Entity::Arc(image) = arc.transformed(&mirror) else {
            panic!("an arc stays an arc");
        };
        // Counter-clockwise from (0,10) to (-10,0): the second quadrant.
        assert!((image.start_angle - FRAC_PI_2).abs() < 1e-15);
        assert!((image.end_angle - PI).abs() < 1e-15);
        assert!(
            (image.arc().sweep - FRAC_PI_2).abs() < 1e-15,
            "not the three-quarter arc"
        );
    }

    #[test]
    fn scaling_scales_radii_and_keeps_arc_angles() {
        let scale = Transform::scaling(Point2::ORIGIN, 3.0).unwrap();
        let circle: Entity = CircleEntity::new(Point2::new(1.0, 2.0), 4.0).into();
        assert_eq!(
            circle.transformed(&scale),
            CircleEntity::new(Point2::new(3.0, 6.0), 12.0).into()
        );
        let arc: Entity = ArcEntity::new(Point2::new(1.0, 2.0), 4.0, 0.3, 2.1).into();
        assert_eq!(
            arc.transformed(&scale),
            ArcEntity::new(Point2::new(3.0, 6.0), 12.0, 0.3, 2.1).into()
        );
    }

    // ── polyline arc segments (#114) ──

    /// A slot from (0,0) to (10,4): two straight sides and two semicircular
    /// ends of radius 2, bulging out to x = -2 and x = 12.
    fn slot() -> PolylineEntity {
        PolylineEntity::with_bulges(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 4.0),
                Point2::new(0.0, 4.0),
            ],
            vec![0.0, 1.0, 0.0, 1.0],
            true,
        )
    }

    #[test]
    fn bulges_have_one_canonical_form() {
        let corners = || {
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
            ]
        };
        let plain = PolylineEntity::new(corners(), false);
        assert!(!plain.has_arcs());
        assert_eq!(plain.bulges().collect::<Vec<_>>(), vec![0.0, 0.0, 0.0]);

        // Zeros, a bulge on a segment that does not exist, and non-numbers
        // all mean "straight", and compare equal to a plain polyline.
        for bulges in [
            vec![],
            vec![0.0, 0.0],
            vec![0.0, 0.0, 0.5],
            vec![f64::NAN, 1e-300],
        ] {
            assert_eq!(
                PolylineEntity::with_bulges(corners(), bulges.clone(), false),
                plain,
                "{bulges:?}"
            );
        }
        // Closed, the last vertex does start a segment.
        let closed = PolylineEntity::with_bulges(corners(), vec![0.0, 0.0, 0.5], true);
        assert!(closed.has_arcs());
        assert_eq!(closed.bulge(2), 0.5);
        assert_eq!(closed.bulge(99), 0.0);
        // Trailing zeros are not stored, so equal shapes are equal values.
        assert_eq!(
            PolylineEntity::with_bulges(corners(), vec![0.5, 0.0, 0.0], true),
            PolylineEntity::with_bulges(corners(), vec![0.5], true)
        );
    }

    /// Regression: a circle drawn as two arcs — a closed polyline of two
    /// vertices — keeps both arcs. (Found by the polyline offset corpus.)
    #[test]
    fn a_two_vertex_closed_polyline_with_arcs_is_a_whole_loop() {
        let ends = || vec![Point2::new(0.0, 0.0), Point2::new(100.0, 0.0)];
        let circle = PolylineEntity::with_bulges(ends(), vec![1.0, 1.0], true);
        assert_eq!(circle.bulges().collect::<Vec<_>>(), vec![1.0, 1.0]);
        let entity = Entity::from(circle);
        assert_eq!(entity.primitives(Tables::none()).len(), 2);
        assert_eq!(
            entity.bounds(Tables::none()),
            Aabb::from_points(Point2::new(0.0, -50.0), Point2::new(100.0, 50.0))
        );
        assert!(
            entity.distance_to(Point2::new(50.0, 50.0), Tables::none()) < 1e-12,
            "the closing arc is there"
        );
        assert!(entity.distance_to(Point2::new(50.0, -50.0), Tables::none()) < 1e-12);

        // Without an arc, two vertices are one line, closed or not.
        let line = Entity::from(PolylineEntity::new(ends(), true));
        assert_eq!(line.primitives(Tables::none()).len(), 1);
        // A bulge on the closing segment alone is enough.
        let lens = Entity::from(PolylineEntity::with_bulges(ends(), vec![0.0, 0.5], true));
        assert_eq!(lens.primitives(Tables::none()).len(), 2);
    }

    #[test]
    fn an_arc_segment_reaches_beyond_the_vertices() {
        let slot = Entity::from(slot());
        assert_eq!(
            slot.bounds(Tables::none()),
            Aabb::from_points(Point2::new(-2.0, 0.0), Point2::new(12.0, 4.0))
        );
        // On the right end's arc, 2 from its centre (10, 2).
        assert!(slot.distance_to(Point2::new(12.0, 2.0), Tables::none()) < 1e-12);
        // The chord between vertices 1 and 2 is not part of the polyline.
        assert!((slot.distance_to(Point2::new(10.0, 2.0), Tables::none()) - 2.0).abs() < 1e-12);
        assert_eq!(slot.primitives(Tables::none()).len(), 4);
        assert_eq!(
            slot.primitives(Tables::none())
                .iter()
                .filter(|primitive| matches!(primitive, Primitive::Arc(_)))
                .count(),
            2
        );
    }

    #[test]
    fn selection_boxes_see_the_arc_not_the_chord() {
        let slot = Entity::from(slot());
        // A box over the bulge of the right end only.
        let over_the_bulge = Aabb::from_points(Point2::new(11.5, 1.5), Point2::new(13.0, 2.5));
        assert!(slot.crosses(&over_the_bulge, Tables::none()));
        // A box over the chord, inside the bulge: nothing is there.
        let over_the_chord = Aabb::from_points(Point2::new(9.8, 1.5), Point2::new(10.4, 2.5));
        assert!(!slot.crosses(&over_the_chord, Tables::none()));
        // Window selection needs the bulges inside too.
        let vertices_only = Aabb::from_points(Point2::new(-0.5, -0.5), Point2::new(10.5, 4.5));
        assert!(!slot.is_inside(&vertices_only, Tables::none()));
        assert!(slot.is_inside(&vertices_only.expanded(2.0), Tables::none()));
    }

    #[test]
    fn other_entities_intersect_the_arc_segments() {
        let slot = Entity::from(slot());
        // A horizontal line through the middle crosses both semicircular ends.
        let through: Entity =
            LineEntity::new(Point2::new(-20.0, 2.0), Point2::new(20.0, 2.0)).into();
        let mut hits = slot.intersections(&through, Tables::none());
        hits.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(hits.len(), 2);
        assert!(hits[0].distance_to(Point2::new(-2.0, 2.0)) < 1e-9);
        assert!(hits[1].distance_to(Point2::new(12.0, 2.0)) < 1e-9);
    }

    #[test]
    fn a_mirror_negates_bulges_and_other_transforms_keep_them() {
        let slot = Entity::from(slot());
        let bulges = |entity: &Entity| match entity {
            Entity::Polyline(polyline) => polyline.bulges().collect::<Vec<_>>(),
            other => panic!("expected a polyline, got {other:?}"),
        };
        let about = Point2::new(3.0, -2.0);
        for kept in [
            Transform::translation(Vec2::new(7.0, 1.0)),
            Transform::rotation(about, 37.0),
            Transform::scaling(about, 2.5).unwrap(),
        ] {
            assert_eq!(bulges(&slot.transformed(&kept)), vec![0.0, 1.0, 0.0, 1.0]);
        }
        let mirror = Transform::mirror(about, Point2::new(8.0, 5.0)).unwrap();
        assert_eq!(
            bulges(&slot.transformed(&mirror)),
            vec![0.0, -1.0, 0.0, -1.0]
        );
        assert_eq!(
            bulges(&slot.transformed(&mirror).transformed(&mirror)),
            vec![0.0, 1.0, 0.0, 1.0]
        );
        // A move is still exact.
        let offset = Vec2::new(100.0, -50.0);
        assert_eq!(slot.translated(offset).translated(-offset), slot);
    }

    mod bulged_polylines {
        use proptest::prelude::*;

        use super::*;

        fn coordinate() -> impl Strategy<Value = f64> {
            -500.0..500f64
        }

        fn point() -> impl Strategy<Value = Point2> {
            (coordinate(), coordinate()).prop_map(|(x, y)| Point2::new(x, y))
        }

        /// A polyline of three to six well-separated vertices with a mix of
        /// straight and arc segments.
        fn polyline() -> impl Strategy<Value = PolylineEntity> {
            (3..7usize, any::<bool>()).prop_flat_map(|(count, closed)| {
                (
                    proptest::collection::vec(point(), count),
                    proptest::collection::vec(prop_oneof![Just(0.0), -3.0..3.0f64], count),
                )
                    .prop_filter("vertices apart", |(vertices, _)| {
                        vertices.iter().enumerate().all(|(index, a)| {
                            vertices[index + 1..]
                                .iter()
                                .all(|b| a.distance_to(*b) > 1.0)
                        })
                    })
                    .prop_map(move |(vertices, bulges)| {
                        PolylineEntity::with_bulges(vertices, bulges, closed)
                    })
            })
        }

        /// Points along every segment: both ends and the middle.
        fn samples(polyline: &PolylineEntity) -> Vec<Point2> {
            polyline
                .primitives()
                .flat_map(|primitive| match primitive {
                    Primitive::Segment(segment) => {
                        vec![
                            segment.start,
                            segment.start + (segment.end - segment.start) * 0.5,
                            segment.end,
                        ]
                    }
                    Primitive::Arc(arc) => {
                        vec![arc.start_point(), arc.mid_point(), arc.end_point()]
                    }
                    Primitive::Circle(circle) => vec![circle.center],
                })
                .collect()
        }

        proptest! {
            /// #114: transforms preserve the shape — the image of every point
            /// of the polyline lies on the image of the polyline.
            #[test]
            fn transforms_carry_every_point_onto_the_image(
                polyline in polyline(), about in point(), other in point(),
                degrees in -360.0..360f64, factor in 0.2..5f64,
            ) {
                prop_assume!(about.distance_to(other) > 1.0);
                let entity = Entity::from(polyline.clone());
                let transforms = [
                    Transform::translation(other - about),
                    Transform::rotation(about, degrees),
                    Transform::scaling(about, factor).unwrap(),
                    Transform::mirror(about, other).unwrap(),
                ];
                for transform in transforms {
                    let image = entity.transformed(&transform);
                    let Entity::Polyline(image_polyline) = &image else {
                        return Err(TestCaseError::fail("a polyline stays a polyline"));
                    };
                    prop_assert_eq!(image_polyline.vertices.len(), polyline.vertices.len());
                    prop_assert_eq!(image.primitives(Tables::none()).len(), entity.primitives(Tables::none()).len());
                    let size = image.bounds(Tables::none()).min().distance_to(image.bounds(Tables::none()).max());
                    for point in samples(&polyline) {
                        let off = image.distance_to(transform.apply(point), Tables::none());
                        prop_assert!(off < 1e-6 * (1.0 + size), "{:?}: off by {}", transform, off);
                    }
                }
            }

            /// Mirroring twice gives back the same polyline.
            #[test]
            fn a_double_mirror_restores_the_polyline(polyline in polyline(), about in point(), other in point()) {
                prop_assume!(about.distance_to(other) > 1.0);
                let mirror = Transform::mirror(about, other).unwrap();
                let entity = Entity::from(polyline.clone());
                let Entity::Polyline(twice) = entity.transformed(&mirror).transformed(&mirror) else {
                    return Err(TestCaseError::fail("a polyline stays a polyline"));
                };
                prop_assert_eq!(twice.bulges().collect::<Vec<_>>(), polyline.bulges().collect::<Vec<_>>());
                for (a, b) in twice.vertices.iter().zip(&polyline.vertices) {
                    prop_assert!(a.distance_to(*b) < 1e-6);
                }
            }

            /// The bounding box holds every sample point, arcs included.
            #[test]
            fn bounds_contain_the_whole_polyline(polyline in polyline()) {
                let entity = Entity::from(polyline.clone());
                let bounds = entity.bounds(Tables::none()).expanded(1e-6);
                for point in samples(&polyline) {
                    prop_assert!(bounds.contains_point(point), "{:?} outside {:?}", point, bounds);
                    prop_assert!(entity.distance_to(point, Tables::none()) < 1e-6);
                }
            }
        }
    }

    #[test]
    fn translation_moves_every_kind_and_keeps_its_shape() {
        let offset = Vec2::new(100.0, -50.0);
        for entity in one_of_each() {
            let moved = entity.translated(offset);
            assert_eq!(
                moved.bounds(Tables::none()).min(),
                entity.bounds(Tables::none()).min() + offset,
                "{entity:?}"
            );
            assert_eq!(
                moved.bounds(Tables::none()).max(),
                entity.bounds(Tables::none()).max() + offset,
                "{entity:?}"
            );
            assert_eq!(
                moved.translated(-offset),
                entity,
                "moving back restores it exactly"
            );
        }
    }

    #[test]
    fn window_selection_needs_the_whole_entity_and_crossing_any_part() {
        let around_everything =
            Aabb::from_points(Point2::new(-50.0, -50.0), Point2::new(50.0, 50.0));
        let far_away = Aabb::from_points(Point2::new(500.0, 500.0), Point2::new(600.0, 600.0));
        for entity in one_of_each() {
            assert!(
                entity.is_inside(&around_everything, Tables::none())
                    && entity.crosses(&around_everything, Tables::none()),
                "{entity:?}"
            );
            assert!(
                !entity.is_inside(&far_away, Tables::none())
                    && !entity.crosses(&far_away, Tables::none()),
                "{entity:?}"
            );
        }

        // A box over the right half of the line: crossing, but not inside.
        let line = &one_of_each()[0];
        let right_half = Aabb::from_points(Point2::new(5.0, -1.0), Point2::new(20.0, 1.0));
        assert!(
            line.crosses(&right_half, Tables::none())
                && !line.is_inside(&right_half, Tables::none())
        );

        // A box inside the circle touches nothing.
        let circle = &one_of_each()[1];
        let inside_disc = Aabb::from_points(Point2::new(4.5, 4.5), Point2::new(5.5, 5.5));
        assert!(!circle.crosses(&inside_disc, Tables::none()));
    }

    #[test]
    fn entities_intersect_through_their_primitives() {
        let all = one_of_each();
        let (line, circle, triangle) = (&all[0], &all[1], &all[3]);
        assert_eq!(
            all.iter()
                .map(|entity| entity.primitives(Tables::none()).len())
                .collect::<Vec<_>>(),
            vec![1, 1, 1, 3, 0, 3]
        );

        // A vertical line through the circle's centre crosses it twice.
        let vertical: Entity =
            LineEntity::new(Point2::new(5.0, -10.0), Point2::new(5.0, 20.0)).into();
        let mut hits = vertical.intersections(circle, Tables::none());
        hits.sort_by(|a, b| a.y.total_cmp(&b.y));
        assert_eq!(hits.len(), 2);
        assert!(
            hits[0].distance_to(Point2::new(5.0, 2.0)) < 1e-9
                && hits[1].distance_to(Point2::new(5.0, 8.0)) < 1e-9
        );

        // It also crosses the bottom line once.
        assert_eq!(vertical.intersections(line, Tables::none()).len(), 1);
        // A vertical through the closed triangle crosses its base and its hypotenuse.
        let through: Entity =
            LineEntity::new(Point2::new(2.0, -10.0), Point2::new(2.0, 20.0)).into();
        assert_eq!(triangle.intersections(&through, Tables::none()).len(), 2);
        // Symmetric.
        assert_eq!(circle.intersections(&vertical, Tables::none()).len(), 2);
    }

    #[test]
    fn as_line_distinguishes_entity_kinds() {
        assert!(line().as_line().is_some());
        assert!(
            Entity::from(CircleEntity::new(Point2::ORIGIN, 1.0))
                .as_line()
                .is_none()
        );
    }
}
