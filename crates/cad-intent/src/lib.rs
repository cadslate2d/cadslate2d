//! The intent IR (ADR-0037, #253): the one language a model may write to
//! change a drawing. A document of a known version, a unit, and
//! operations from a catalogue — draw, modify, layers and properties,
//! dimension — with references to existing objects by the stable IDs the
//! context gave out.
//!
//! Nothing here touches a drawing. [`parse`] reads JSON and validates it
//! against the limits; [`compile`] turns each operation into a registered
//! command and its typed answers, which the engine runs through its one
//! entry point, exactly as a menu macro is run (ADR-0034). File, host, and
//! settings commands are not in the catalogue. The JSON Schema of the
//! document is generated from these types (`schema`, behind the `schema`
//! feature) for a model's structured output, and snapshot-tested: a
//! change to it bumps [`VERSION`].

use cad_core::{LengthUnit, Point2};
use serde::{Deserialize, Serialize};

mod compile;
mod validate;

mod datums;
pub use compile::{Answer, COMMANDS, Compiled, Step, compile};
pub use datums::unused_datums;
pub use validate::{IntentError, Reason, limits};

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// The version of the document this crate writes and reads. Bumped when
/// the schema changes.
pub const VERSION: u32 = 24;

/// The versions this crate reads: each a subset of the next, so a
/// document of an earlier one is read as it was meant. Version 2 added
/// the parts (#272), version 3 the placements (#273), version 4 the
/// constraints and parameters (#274), version 5 floor plans (#282),
/// version 6 their windows (#324), version 7 `erase` (#325), version 8
/// `dim_scale` (#327), version 9 `members` (#329), version 10 `stair` and
/// a brief's stair room and storey height (#331), version 11 the
/// isometric operations (#336), version 12 `elevation` and a brief's
/// `building` (#337); `building_section` (#338) came with it; version 13
/// `part_views` (#339) and `title_block` (#340), version 14 `spline` and
/// `hatch` (#345), version 15 `view` (#351, ADR-0041), version 16 a free
/// view's `ratio` (#352), version 17 a section view's `cut` (#353),
/// version 18 `bill_of_materials` and `balloon` (#354), version 19
/// `dim_chain` and `notes` (#355), version 20 a layer's `color` and
/// `linetype` (#356), version 21 `datum` and a coordinate given as one
/// (#361), version 22 a section or free view `looking` `from_front`
/// (#358), version 23 `segments` and a layer's `lineweight` (#358),
/// version 24 a view's `scale` as written, "1:20" (#358).
pub const READS: [u32; 24] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
];

/// A point, `[x, y]`, in the document's units.
pub type Point = [f64; 2];

/// A point on the isometric axes (#336), `[x, y, z]`: `x` along 30°, `y`
/// along 150°, `z` up the page, each at true length from the origin.
pub type IsoPoint = [f64; 3];

/// What a model asks the engine to do: a version, a unit the numbers are
/// in, and the operations in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Intent {
    /// The schema version; this crate reads [`VERSION`].
    pub version: u32,
    /// The unit every length and coordinate is in; the drawing's own when
    /// absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<Unit>,
    /// The operations, run in order; each one's new objects are what the
    /// next may refer to as the selection.
    pub operations: Vec<Operation>,
}

/// A length unit the numbers are given in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Mm,
    Cm,
    M,
    In,
    Ft,
}

impl Unit {
    /// The drawing unit this is.
    pub fn length_unit(self) -> LengthUnit {
        match self {
            Self::Mm => LengthUnit::Millimetres,
            Self::Cm => LengthUnit::Centimetres,
            Self::M => LengthUnit::Metres,
            Self::In => LengthUnit::Inches,
            Self::Ft => LengthUnit::Feet,
        }
    }
}

/// Which existing objects an operation acts on: the current selection —
/// what the user had selected, or what the previous operation made — or
/// objects by the IDs the context gave out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum Targets {
    /// `"selection"`.
    Selection(SelectionWord),
    /// Object IDs, at least one.
    Ids(Vec<u64>),
}

/// The one word [`Targets::Selection`] takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum SelectionWord {
    Selection,
}

/// One object picked where a command asks for one, and, where it matters
/// which part — the part of a line a trim removes, the side an offset goes
/// to — the point the pick is nearest to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Pick {
    /// The object's ID, as the context gave it out.
    pub id: u64,
    /// A point near the part meant; the object's nearest point to it is
    /// where the pick lands. Any point of the object when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near: Option<Point>,
}

/// Where a linear dimension's line runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// An isometric plane (#336): the face of a box two axes span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum IsoPlane {
    /// `y` and `z`.
    Left,
    /// `x` and `y`.
    #[default]
    Top,
    /// `x` and `z`.
    Right,
}

impl IsoPlane {
    pub fn plane(self) -> cad_core::iso::Plane {
        match self {
            Self::Left => cad_core::iso::Plane::Left,
            Self::Top => cad_core::iso::Plane::Top,
            Self::Right => cad_core::iso::Plane::Right,
        }
    }
}

/// A building's side, as an elevation sees it (#337): the front faces the
/// street, along the plot's width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Front,
    Back,
    Left,
    Right,
}

/// A roof's kind (#337).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum RoofKind {
    /// A slab behind a parapet.
    Flat,
    /// Two slopes, gable ends on the short sides.
    Gable,
    /// Four slopes.
    Hip,
}

/// A part to draw the views of (#339): a prism — a front `profile` of
/// `[x, z]` corners extruded `depth` back, with `holes` — or a turned part
/// — `steps` of `[diameter, length]` along its axis, with a `bore`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PartDescription {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<Vec<Point>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<PartHole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<[f64; 2]>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bore: Option<f64>,
}

/// A hole right through a prism along `axis`: `at` is `[x, z]` along y,
/// `[x, y]` along z, `[y, z]` along x.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PartHole {
    pub axis: PartAxis,
    pub at: Point,
    pub diameter: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum PartAxis {
    X,
    Y,
    Z,
}

/// How a part's views are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Projection {
    /// The top view under the front, the right side on its left (ISO).
    #[default]
    First,
    /// The top view over the front, the right side on its right (ASME).
    Third,
}

/// A sheet's paper (#340).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PaperSize {
    A4,
    #[default]
    A3,
    A2,
    A1,
    A0,
}

/// A stair's kind (#331).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum StairKind {
    #[default]
    Straight,
    /// Turning a quarter to the left at a landing.
    L,
    /// Turning back to the left at a landing.
    U,
    /// Round a newel, counter-clockwise going up.
    Helical,
}

/// How a stair is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum StairView {
    /// Seen from above, cut 1.2 m up.
    #[default]
    Plan,
    /// The profile of its risers and treads, the floors below and above.
    Section,
}

/// A geometric constraint's kind (ADR-0038).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    /// Two points at one place: `first` and `second` each near an end, a
    /// centre, or a vertex.
    Coincident,
    /// Two lines on one line.
    Collinear,
    /// Two circles or arcs about one centre.
    Concentric,
    /// A point kept where it is: `first` near an end, a centre, or a vertex.
    Fix,
    Parallel,
    Perpendicular,
    /// A line level: `first` only.
    Horizontal,
    /// A line plumb: `first` only.
    Vertical,
    /// A line or a circle touching a circle or an arc.
    Tangent,
    /// Two lines of one length, or two circles or arcs of one radius.
    Equal,
}

impl Relation {
    /// Whether it relates a part of one object only.
    pub fn takes_one(self) -> bool {
        matches!(self, Self::Fix | Self::Horizontal | Self::Vertical)
    }
}

/// What a dimensional constraint measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Measure {
    /// Along x.
    Horizontal,
    /// Along y.
    Vertical,
    /// Point to point.
    Aligned,
    /// The angle between two lines, degrees.
    Angular,
    Radius,
    Diameter,
}

impl Measure {
    /// Whether it measures between two points.
    pub fn is_linear(self) -> bool {
        matches!(self, Self::Horizontal | Self::Vertical | Self::Aligned)
    }
}

/// A colour: an index of the 1–255 table, or a name the engine knows
/// (`red`, `bylayer`, …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum Color {
    Index(u16),
    Name(String),
}

/// One operation of the catalogue. Every length and coordinate is in the
/// document's units; every angle is in degrees, counter-clockwise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    // ── draw ──
    /// Line segments through the points, in order: one object per segment,
    /// each joined to the next — a chain.
    Line {
        points: Vec<Point>,
    },
    /// Separate line segments, each its own pair of points, joined to
    /// nothing: a row of parcel boundaries, stripes, curbs (version 23,
    /// #358). A `line` through such points would join each to the next.
    Segments {
        segments: Vec<[Point; 2]>,
    },
    /// One polyline through the points, closed back to the first or not.
    Polyline {
        points: Vec<Point>,
        #[serde(default)]
        closed: bool,
    },
    /// A smooth curve through the points, in order: an interpolating
    /// spline, passing through every one exactly.
    Spline {
        points: Vec<Point>,
    },
    /// A filled region: `SOLID`, or a pattern by name (`ANSI31` for a cut
    /// section, `EARTH` for ground, `BRICK`, `STEEL`, …) at an angle and a
    /// scale. The boundary is the smallest closed loop around
    /// `inside`, or every loop the `targets` make: one of the two.
    Hatch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inside: Option<Point>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        targets: Option<Targets>,
        #[serde(default = "default_solid")]
        pattern: String,
        #[serde(default)]
        angle: f64,
        #[serde(default = "default_one")]
        scale: f64,
    },
    /// A view of a composed sheet (ADR-0041): its `operations` draw it in
    /// its own coordinates, the object's reference point at the origin; the
    /// program then places it on the sheet the last `title_block` drew,
    /// aligned with its parent when `projected`, and titles it. Views given
    /// one after another are placed together. A view holds no view.
    View {
        id: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
        placed: ViewPlacement,
        /// The view a projected view is beside.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        of: Option<String>,
        /// Where a projected view looks from.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        looking: Option<ViewLooking>,
        /// A free view's own scale, paper to real: 2 for a detail at 2:1.
        /// Its operations still draw true size; the program enlarges the
        /// geometry and its dimensions still read true. The sheet's scale
        /// when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ratio: Option<f64>,
        /// A free view's own scale as a drafter writes it, paper to real:
        /// `"1:20"`, `"2:1"` — the view's own, never relative to the
        /// sheet's (version 24, #358). Read into `ratio` when the document
        /// is read; one or the other.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<String>,
        /// A section view's cut: across the view `of`, `from` and `to` in
        /// that view's own coordinates — its ends may reach past the
        /// outline, but it must cross what that view draws — its arrows
        /// looking toward `toward`. The program draws the marker and the
        /// letter, and titles the section to match.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cut: Option<SectionCut>,
        operations: Vec<Operation>,
    },
    /// A composed sheet's bill of materials (ADR-0041 §5): its parts drawn
    /// as a table — item, name, quantity, material — and placed on the
    /// sheet where there is room.
    BillOfMaterials {
        parts: Vec<BomPart>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
    },
    /// A balloon: part `part` of the bill of materials, a leader from `at`
    /// on the part to the balloon at `to`, both in the view `view`'s own
    /// coordinates. A part the bill does not have is refused.
    Balloon {
        part: u32,
        view: String,
        at: Point,
        to: Point,
    },
    /// A chain of linear dimensions between `points`, in order, along one
    /// side of what is drawn: the program puts the dimension line beyond
    /// the geometry on that side, and beyond the chains already there.
    DimChain {
        points: Vec<Point>,
        side: ChainSide,
    },
    /// A block of notes on the sheet: its lines, wrapped to `width` paper
    /// millimetres (120 when absent), placed where there is room.
    Notes {
        lines: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f64>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
    },
    /// A datum of the object a composed sheet shows (ADR-0041 §3): a
    /// named value along its width, depth, or height. Any coordinate of an
    /// operation inside a view may be `{"datum": name}` instead of a
    /// number; it stands for the value, with the sign the view's
    /// orientation gives — a width read from behind is negated, a depth
    /// read from the left too. `views` names the views the feature it marks
    /// must be drawn in; each is refused unless it uses it.
    Datum {
        name: String,
        axis: DatumAxis,
        value: f64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        views: Vec<String>,
    },
    /// A closed rectangle from two opposite corners.
    Rectangle {
        corner: Point,
        opposite: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    /// An arc through three points: where it starts, a point on it, where it ends.
    Arc {
        start: Point,
        through: Point,
        end: Point,
    },
    /// A regular polygon as a closed polyline: the corners on the circle
    /// (inscribed, the default), or the sides touching it.
    Polygon {
        sides: u32,
        center: Point,
        radius: f64,
        #[serde(default)]
        circumscribed: bool,
    },
    /// A single line of text at a point; the current text style's height
    /// when none is given, and no rotation.
    Text {
        at: Point,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rotation: Option<f64>,
    },

    /// An isometric drawing's lines (#336): segments through the points on
    /// the isometric axes, in order, one object per segment.
    IsoLine {
        points: Vec<IsoPoint>,
    },
    /// One polyline through points on the isometric axes.
    IsoPolyline {
        points: Vec<IsoPoint>,
        #[serde(default)]
        closed: bool,
    },
    /// A circle on an isometric plane, drawn as the ellipse it is: a hole
    /// in a box's top face is on `top`, in its right face on `right`.
    IsoCircle {
        center: IsoPoint,
        radius: f64,
        #[serde(default)]
        plane: IsoPlane,
    },

    // ── modify ──
    /// Lines made into members with a width (#329): each group's lines as
    /// wide as its `width`, meeting every other group's — continuous
    /// through a joint along them, butted against another's face, mitred
    /// where they end together — as closed outlines; the centrelines go
    /// unless `keep_centrelines`.
    Members {
        groups: Vec<MemberGroup>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        keep_centrelines: bool,
    },
    /// Removes the targets from the drawing: what a plan drawn earlier
    /// is, before it is drawn again revised (#325).
    Erase {
        targets: Targets,
    },
    /// Moves the targets by a displacement.
    Move {
        targets: Targets,
        displacement: Point,
    },
    /// Copies the targets, the copies displaced; the originals stay.
    Copy {
        targets: Targets,
        displacement: Point,
    },
    /// Rotates the targets about a base point by an angle in degrees.
    Rotate {
        targets: Targets,
        base: Point,
        angle: f64,
    },
    /// Scales the targets uniformly about a base point.
    Scale {
        targets: Targets,
        base: Point,
        factor: f64,
    },
    /// Reflects the targets across the line through two points; the
    /// originals are kept unless `keep` is false.
    Mirror {
        targets: Targets,
        axis: [Point; 2],
        #[serde(default = "default_true")]
        keep: bool,
    },
    /// A parallel copy of one object at a distance, on the side of a point.
    Offset {
        target: Pick,
        distance: f64,
        toward: Point,
    },
    /// Trims objects at the cutting edges — every object when `edges` is
    /// absent — removing the part of each target nearest its pick.
    Trim {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        edges: Option<Targets>,
        targets: Vec<Pick>,
    },
    /// Extends objects to the boundary edges — every object when `edges`
    /// is absent — the end of each target nearest its pick.
    Extend {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        edges: Option<Targets>,
        targets: Vec<Pick>,
    },
    /// Joins two objects with a tangent arc of the radius; zero makes a
    /// sharp corner. The picks say which parts are kept.
    Fillet {
        radius: f64,
        first: Pick,
        second: Pick,
    },
    /// Joins two lines with a straight bevel, the distances along each.
    Chamfer {
        first_distance: f64,
        second_distance: f64,
        first: Pick,
        second: Pick,
    },
    /// Copies of the targets in rows and columns; a negative spacing goes
    /// the other way.
    Array {
        targets: Targets,
        rows: u32,
        columns: u32,
        row_spacing: f64,
        column_spacing: f64,
    },
    /// Copies of the targets round a centre: how many, over what angle
    /// (360 by default, negative clockwise), turned as they go round or not.
    PolarArray {
        targets: Targets,
        center: Point,
        count: u32,
        #[serde(default = "default_full_turn")]
        angle: f64,
        #[serde(default = "default_true")]
        rotate: bool,
    },

    // ── layers and properties ──
    /// The scale the current dimension style draws every size at — its
    /// text, arrowheads, gaps — and never the value (DIMSCALE, #327): text
    /// 2.5 high at 1, 10 high at 4.
    DimScale {
        scale: f64,
    },
    /// A layer by name, made if it does not exist, and made current when
    /// asked; given a colour, a linetype, and a lineweight when asked —
    /// `CENTER` for centre lines, `HIDDEN` for hidden edges, `PHANTOM`,
    /// `DASHED`, loaded from the program's own when the drawing has none of
    /// that name; a lineweight in millimetres, `0.5` for outlines (version
    /// 23, #358).
    Layer {
        name: String,
        #[serde(default)]
        current: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<Color>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        linetype: Option<String>,
        /// A lineweight in millimetres, or `default`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lineweight: Option<String>,
    },
    /// Changes the layer, colour, linetype, or lineweight of the targets;
    /// whatever is given, at least one.
    SetProperties {
        targets: Targets,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<Color>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        linetype: Option<String>,
        /// A lineweight in millimetres, or `default`, `bylayer`, `byblock`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lineweight: Option<String>,
    },

    // ── dimension ──
    /// A horizontal or vertical dimension between two points, its line
    /// through `at`; horizontal or vertical by where `at` is unless said.
    DimLinear {
        from: Point,
        to: Point,
        at: Point,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        orientation: Option<Orientation>,
    },
    /// A dimension measured along the two points, its line through `at`.
    DimAligned {
        from: Point,
        to: Point,
        at: Point,
    },
    /// The radius of an arc or circle, the text toward `at`.
    DimRadius {
        target: Pick,
        at: Point,
    },
    /// The diameter of an arc or circle, the text toward `at`.
    DimDiameter {
        target: Pick,
        at: Point,
    },
    /// The angle at a vertex between two points, the arc through `at`.
    DimAngular {
        vertex: Point,
        first: Point,
        second: Point,
        at: Point,
    },

    // ── parts (#272): constrained geometry, made as one ──
    /// A rectangular plate from its lower left corner, its sides and size
    /// constrained: corners square, rounded by `fillet`, or bevelled by
    /// `chamfer`; with `hole_diameter`, a hole at each corner,
    /// `hole_offset` in from both edges.
    Plate {
        corner: Point,
        width: f64,
        height: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fillet: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chamfer: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hole_diameter: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hole_offset: Option<f64>,
    },
    /// A slot: its centres `length` apart along `angle`, `width` across.
    Slot {
        center: Point,
        length: f64,
        width: f64,
        #[serde(default)]
        angle: f64,
    },
    /// Holes in rows and columns from the first, lower left one; a
    /// negative spacing goes the other way.
    HolePattern {
        first: Point,
        diameter: f64,
        rows: u32,
        columns: u32,
        #[serde(default)]
        row_spacing: f64,
        #[serde(default)]
        column_spacing: f64,
    },
    /// Holes equally spaced on a pitch circle, the first at `start_angle`.
    BoltCircle {
        center: Point,
        pitch_diameter: f64,
        count: u32,
        hole_diameter: f64,
        #[serde(default)]
        start_angle: f64,
    },
    /// A round flange: an outer circle, a bore, and a bolt circle.
    RoundFlange {
        center: Point,
        outer_diameter: f64,
        bore_diameter: f64,
        pitch_diameter: f64,
        count: u32,
        hole_diameter: f64,
    },
    /// A rectangular flange about its centre: a bore, and a hole at each
    /// corner `hole_offset` in from both edges.
    RectangularFlange {
        center: Point,
        width: f64,
        height: f64,
        bore_diameter: f64,
        hole_diameter: f64,
        hole_offset: f64,
    },

    // ── placement (#273): placed against existing objects, kept so ──
    /// A circle tangent to two objects with a `radius`, or to three with
    /// `third` and no radius; of the circles there are, the one touching
    /// nearest the picks. Kept tangent to them.
    TangentCircle {
        first: Pick,
        second: Pick,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        third: Option<Pick>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<f64>,
    },
    /// An arc of a radius tangent to two objects, between where it touches
    /// them, the short way: a fillet that trims nothing. Kept tangent, its
    /// ends on them.
    TangentArc {
        first: Pick,
        second: Pick,
        radius: f64,
    },
    /// A circle about the centre of a circle or an arc, kept concentric.
    ConcentricCircle {
        of: Pick,
        radius: f64,
    },
    /// The targets along a curve, from its end nearest the pick: `count`
    /// of them from end to end — evenly round a closed curve — or one
    /// every `spacing`. The targets move to the first place. A lone circle
    /// along a line, an arc, or a circle is kept on it, and its copies its
    /// size.
    PathArray {
        targets: Targets,
        path: Pick,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spacing: Option<f64>,
    },
    /// Copies of the targets on a grid through them, `column_spacing`
    /// across and `row_spacing` up — the same when absent — wherever a
    /// copy lies wholly inside the closed boundary round `inside`, its
    /// islands left out.
    GridArray {
        targets: Targets,
        inside: Point,
        column_spacing: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row_spacing: Option<f64>,
    },

    // ── constraints (#274): relations kept, and parameters that drive ──
    /// A geometric constraint between parts of objects: the second comes to
    /// the first. A point is meant by `near` at it.
    Constrain {
        relation: Relation,
        first: Pick,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        second: Option<Pick>,
    },
    /// A dimensional constraint, a parameter named `d1`, `ang1`, `rad1`,
    /// `dia1` … that drives the geometry: between two `points` — ends,
    /// centres, or vertices — or along the line `of`, end to end; of the
    /// circle or arc `of`; or the angle from the line `of` to the line
    /// `and`. Shown with its dimension line through `at`; `value`, when
    /// given, is what it is made — the measured one otherwise.
    ConstrainDimension {
        measure: Measure,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        points: Option<[Point; 2]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        of: Option<Pick>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        and: Option<Pick>,
        at: Point,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<f64>,
    },
    /// A parameter set to a new value — a length, or an angle in degrees
    /// for an `ang` one — and the constrained geometry follows.
    SetParameter {
        name: String,
        value: f64,
    },

    // ── floor plans (#282, ADR-0039) ──
    /// A floor plan laid out from a brief and drawn: walls, doors, and
    /// labels. The brief is in metres, whatever the document's unit; `at`
    /// is where the terrain's lower left corner goes (the origin when
    /// absent); `alternative` is which of the plans found, 1 to 3, the
    /// best first. A revision of a plan — a room bigger, two rooms side by
    /// side, other setbacks — is this operation with its brief revised.
    FloorPlan {
        brief: Brief,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Point>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alternative: Option<u32>,
    },
    /// A façade of a floor plan drawn already (#337): `plan` its group's
    /// name, `PLAN-1`; the side it is seen from; the roof — the brief's
    /// when absent — and a pitched roof's `roof_pitch` in degrees; `at`,
    /// where the façade's left end stands on the ground, under the plan
    /// when absent.
    Elevation {
        plan: String,
        #[serde(default)]
        side: Side,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        roof: Option<RoofKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        roof_pitch: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Point>,
    },
    /// A section through a floor plan drawn already (#338): `plan` its
    /// group's name; the cut from `from` to `to`, two points of the
    /// drawing across the plan or up it — the cut is square to the plan's
    /// axis they run nearer, through their middle, and the section looks
    /// to the left of the way from `from` to `to`; `at`, where the floor's
    /// left end goes, under the plan when absent.
    BuildingSection {
        plan: String,
        from: Point,
        to: Point,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Point>,
    },
    /// A part's standard views (#339): the front — cut in section when
    /// `section` — the top unless the part is turned, and the right side,
    /// hidden edges dashed and centre lines drawn; the front view's lower
    /// left corner `at`, the origin when absent.
    PartViews {
        part: PartDescription,
        #[serde(default)]
        projection: Projection,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        section: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Point>,
    },
    /// A sheet's border and title block (#340): the paper, landscape unless
    /// `portrait`; in the model, drawn at 1:`scale` (1 when absent) — on a
    /// layout, in paper millimetres; the fields; the projection; its lower
    /// left corner `at`, the origin when absent.
    TitleBlock {
        #[serde(default)]
        paper: PaperSize,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        portrait: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scale: Option<f64>,
        #[serde(default)]
        title: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        number: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        drawn_by: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        date: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        sheet: String,
        #[serde(default)]
        projection: Projection,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<Point>,
    },
    /// A stair sized by Blondel's rule (#331) — two risers and a tread
    /// make 63 cm, risers at most 18 cm, treads at least 28 — climbing a
    /// floor-to-floor `height`, `width` wide: in plan, its foot's middle
    /// `at` and climbing along `direction` (degrees, 90 when absent); in
    /// section, its foot `at` on the floor below and running along
    /// `direction` (0 when absent). A helix turns round a newel of
    /// `newel`'s radius, 15 cm when absent; only a helix has one.
    Stair {
        height: f64,
        width: f64,
        #[serde(default)]
        kind: StairKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        newel: Option<f64>,
        #[serde(default)]
        view: StairView,
        at: Point,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direction: Option<f64>,
    },
}

/// A floor plan's brief (ADR-0039 §1), in metres: what `FLOORPLAN` reads.
/// Absent fields take the generator's defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Brief {
    /// The plot: `width` along its front (x), `depth` back from it (y).
    pub terrain: BriefTerrain,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setbacks: Option<BriefSetbacks>,
    pub rooms: Vec<BriefRoom>,
    /// Pairs of room names that must share a wall with a door in it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adjacent: Vec<[String; 2]>,
    /// The most floor the search may add as a hall, as a share of the
    /// rooms' floor; 0 forbids it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_circulation: Option<f64>,
    /// The windows' width, metres: a window in each outside wall of every
    /// room. Absent, only rooms that ask for an outside wall have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<f64>,
    /// The floor-to-floor height a room's stair climbs, metres; 2.8 when
    /// absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storey_height: Option<f64>,
    /// The heights and roof its elevations and sections are drawn with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building: Option<BriefBuilding>,
}

/// What a building's elevations and sections need besides its plan, in
/// metres and degrees; each absent field the generator's default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BriefBuilding {
    /// A window's sill above the floor; 0.9.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sill: Option<f64>,
    /// A window's head; 2.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_head: Option<f64>,
    /// A door's head; 2.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door_head: Option<f64>,
    /// The slabs' thickness; 0.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slab: Option<f64>,
    /// Flat when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roof: Option<RoofKind>,
    /// A pitched roof's slope, degrees; 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roof_pitch: Option<f64>,
    /// How far a pitched roof reaches past the walls; 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eaves: Option<f64>,
    /// A flat roof's parapet above its slab; 0.5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parapet: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BriefTerrain {
    pub width: f64,
    pub depth: f64,
}

/// How far the building keeps from each edge of the plot; the front is
/// along the x axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BriefSetbacks {
    #[serde(default)]
    pub front: f64,
    #[serde(default)]
    pub back: f64,
    #[serde(default)]
    pub left: f64,
    #[serde(default)]
    pub right: f64,
}

/// Lines of one width, made members together (#329).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MemberGroup {
    pub targets: Targets,
    pub width: f64,
}

/// A room: its name and its floor area inside its walls, m².
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BriefRoom {
    pub name: String,
    pub area: f64,
    /// Its narrower side is at least this; 1.5 m when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    /// Its longer side over its shorter is at most this; 2.5 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_aspect: Option<f64>,
    /// It wants an outside wall, for a window.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exterior: bool,
    /// The way in; the first room when none says so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub entrance: bool,
    /// It holds the stair to the floor above: made big enough for one and
    /// the stair drawn in it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stair: bool,
}

fn default_true() -> bool {
    true
}

/// How a view is placed on its sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ViewPlacement {
    /// The front: the root of the projection, one per sheet.
    Principal,
    /// Beside the view `of`, looking from a side.
    Projected,
    /// Wherever there is room: an exploded view, a detail.
    Free,
    /// A section: placed where there is room, its `cut` marked across the
    /// view it cuts.
    Section,
}

/// An axis of the object a composed sheet shows: width to the right as the
/// front view shows it, depth away from the front, height up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DatumAxis {
    Width,
    Depth,
    Height,
}

/// Which side of the geometry a chain of dimensions runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ChainSide {
    Below,
    Above,
    Left,
    Right,
}

/// The axis a chain of dimensions measures along, 0 for x and 1 for y —
/// the one its points spread further along, whatever side its line is on
/// — and the points it dimensions: each that moves along that axis from
/// the last kept. A point at the same place along it would make a
/// dimension of nothing, which DIMCONTINUE refuses (#358).
pub fn chain(points: &[Point]) -> (usize, Vec<Point>) {
    let spread = |axis: usize| {
        let values = points.iter().map(|p| p[axis]);
        values.clone().fold(f64::NEG_INFINITY, f64::max) - values.fold(f64::INFINITY, f64::min)
    };
    let axis = usize::from(spread(1) > spread(0));
    let mut kept: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        let moves = kept.last().is_none_or(|last| {
            let tolerance = cad_core::tolerance::LinearTolerance::DEFAULT.around(&[
                cad_core::Point2::new(last[0], last[1]),
                cad_core::Point2::new(point[0], point[1]),
            ]);
            !tolerance.is_zero(point[axis] - last[axis])
        });
        if moves {
            kept.push(*point);
        }
    }
    (axis, kept)
}

/// A part of a bill of materials.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BomPart {
    pub number: u32,
    pub name: String,
    pub quantity: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub material: String,
}

/// Where a section view's cut is (ADR-0041 §5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SectionCut {
    pub of: String,
    pub from: Point,
    pub to: Point,
    pub toward: Point,
}

/// Where a view looks at the object from: a projected view from a side,
/// above, below, or behind; a section or a free view from any of those or
/// the front (version 22), which gives its datums their axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ViewLooking {
    FromFront,
    FromLeft,
    FromRight,
    FromAbove,
    FromBelow,
    FromBehind,
}

fn default_solid() -> String {
    "SOLID".to_owned()
}

fn default_one() -> f64 {
    1.0
}

fn default_full_turn() -> f64 {
    360.0
}

impl Operation {
    /// The operation's name in the document: its `op` tag.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Line { .. } => "line",
            Self::Segments { .. } => "segments",
            Self::Polyline { .. } => "polyline",
            Self::Spline { .. } => "spline",
            Self::View { .. } => "view",
            Self::BillOfMaterials { .. } => "bill_of_materials",
            Self::Balloon { .. } => "balloon",
            Self::DimChain { .. } => "dim_chain",
            Self::Datum { .. } => "datum",
            Self::Notes { .. } => "notes",
            Self::Hatch { .. } => "hatch",
            Self::Rectangle { .. } => "rectangle",
            Self::Circle { .. } => "circle",
            Self::Arc { .. } => "arc",
            Self::Polygon { .. } => "polygon",
            Self::Text { .. } => "text",
            Self::IsoLine { .. } => "iso_line",
            Self::IsoPolyline { .. } => "iso_polyline",
            Self::IsoCircle { .. } => "iso_circle",
            Self::Members { .. } => "members",
            Self::Erase { .. } => "erase",
            Self::Move { .. } => "move",
            Self::Copy { .. } => "copy",
            Self::Rotate { .. } => "rotate",
            Self::Scale { .. } => "scale",
            Self::Mirror { .. } => "mirror",
            Self::Offset { .. } => "offset",
            Self::Trim { .. } => "trim",
            Self::Extend { .. } => "extend",
            Self::Fillet { .. } => "fillet",
            Self::Chamfer { .. } => "chamfer",
            Self::Array { .. } => "array",
            Self::PolarArray { .. } => "polar_array",
            Self::DimScale { .. } => "dim_scale",
            Self::Layer { .. } => "layer",
            Self::SetProperties { .. } => "set_properties",
            Self::DimLinear { .. } => "dim_linear",
            Self::DimAligned { .. } => "dim_aligned",
            Self::DimRadius { .. } => "dim_radius",
            Self::DimDiameter { .. } => "dim_diameter",
            Self::DimAngular { .. } => "dim_angular",
            Self::Plate { .. } => "plate",
            Self::Slot { .. } => "slot",
            Self::HolePattern { .. } => "hole_pattern",
            Self::BoltCircle { .. } => "bolt_circle",
            Self::RoundFlange { .. } => "round_flange",
            Self::RectangularFlange { .. } => "rectangular_flange",
            Self::TangentCircle { .. } => "tangent_circle",
            Self::TangentArc { .. } => "tangent_arc",
            Self::ConcentricCircle { .. } => "concentric_circle",
            Self::PathArray { .. } => "path_array",
            Self::GridArray { .. } => "grid_array",
            Self::Constrain { .. } => "constrain",
            Self::ConstrainDimension { .. } => "constrain_dimension",
            Self::SetParameter { .. } => "set_parameter",
            Self::FloorPlan { .. } => "floor_plan",
            Self::Stair { .. } => "stair",
            Self::Elevation { .. } => "elevation",
            Self::BuildingSection { .. } => "building_section",
            Self::PartViews { .. } => "part_views",
            Self::TitleBlock { .. } => "title_block",
        }
    }
}

/// Reads a document from its JSON and validates it: the version, the
/// limits, every number finite and in range, every reference and name
/// well formed. What is refused says where and why ([`IntentError`]).
pub fn parse(text: &str) -> Result<Intent, IntentError> {
    let json_error = |error: serde_json::Error| IntentError::Json {
        line: error.line(),
        column: error.column(),
        message: error.to_string(),
    };
    let mut value: serde_json::Value = serde_json::from_str(text).map_err(json_error)?;
    // Datums are read into the numbers they stand for, so every operation
    // keeps its plain coordinates (#361).
    datums::resolve(&mut value)?;
    // A view's scale as written, "1:20", is read into its ratio (#358).
    read_scales(&mut value)?;
    let intent: Intent = serde_json::from_value(value).map_err(|error| IntentError::Json {
        line: 1,
        column: 1,
        message: error.to_string(),
    })?;
    validate::validate(&intent)?;
    Ok(intent)
}

/// Each view's `scale`, `"1:20"` or `"2:1"`, read into its `ratio`, paper
/// to real; refused when it is not two numbers greater than zero, or comes
/// with a `ratio` of its own.
fn read_scales(document: &mut serde_json::Value) -> Result<(), IntentError> {
    let Some(operations) = document
        .get_mut("operations")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for (index, op) in operations.iter_mut().enumerate() {
        if op["op"] != "view" {
            continue;
        }
        let Some(object) = op.as_object_mut() else {
            continue;
        };
        let Some(scale) = object.remove("scale") else {
            continue;
        };
        if scale.is_null() {
            continue;
        }
        let refuse = |reason: Reason| IntentError::Invalid {
            operation: index,
            op: "view",
            field: "scale",
            reason,
        };
        if object.get("ratio").is_some_and(|r| !r.is_null()) {
            return Err(refuse(Reason::Both("ratio")));
        }
        let ratio = scale.as_str().and_then(|text| {
            let (paper, real) = text.split_once(':')?;
            let paper: f64 = paper.trim().parse().ok()?;
            let real: f64 = real.trim().parse().ok()?;
            (paper > 0.0 && real > 0.0 && (paper / real).is_finite()).then(|| paper / real)
        });
        let Some(ratio) = ratio else {
            return Err(refuse(Reason::Said(
                "is paper to real as a drafter writes it: \"1:20\" for a twentieth, \"2:1\" for twice true size",
            )));
        };
        object.insert("ratio".to_owned(), serde_json::json!(ratio));
    }
    Ok(())
}

/// The JSON Schema of [`Intent`], for a model's structured output
/// (`features = ["schema"]`).
#[cfg(feature = "schema")]
pub fn schema() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(Intent)).expect("a schema is JSON");
    datums::widen_points(&mut schema);
    schema
}

pub(crate) fn point(p: Point) -> Point2 {
    Point2::new(p[0], p[1])
}

/// Where a point on the isometric axes falls on the page.
pub fn iso_point(p: IsoPoint) -> Point {
    let q = cad_core::iso::map(p[0], p[1], p[2]);
    [q.x, q.y]
}
