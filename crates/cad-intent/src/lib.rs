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

pub use compile::{Answer, COMMANDS, Compiled, Step, compile};
pub use validate::{IntentError, Reason, limits};

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// The version of the document this crate writes and reads. Bumped when
/// the schema changes.
pub const VERSION: u32 = 7;

/// The versions this crate reads: each a subset of the next, so a
/// document of an earlier one is read as it was meant. Version 2 added
/// the parts (#272), version 3 the placements (#273), version 4 the
/// constraints and parameters (#274), version 5 floor plans (#282),
/// version 6 their windows (#324), version 7 `erase` (#325).
pub const READS: [u32; 7] = [1, 2, 3, 4, 5, 6, 7];

/// A point, `[x, y]`, in the document's units.
pub type Point = [f64; 2];

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
    /// Line segments through the points, in order: one object per segment.
    Line {
        points: Vec<Point>,
    },
    /// One polyline through the points, closed back to the first or not.
    Polyline {
        points: Vec<Point>,
        #[serde(default)]
        closed: bool,
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

    // ── modify ──
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
    /// A layer by name, made if it does not exist, and made current when
    /// asked.
    Layer {
        name: String,
        #[serde(default)]
        current: bool,
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
}

fn default_true() -> bool {
    true
}

fn default_full_turn() -> f64 {
    360.0
}

impl Operation {
    /// The operation's name in the document: its `op` tag.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Line { .. } => "line",
            Self::Polyline { .. } => "polyline",
            Self::Rectangle { .. } => "rectangle",
            Self::Circle { .. } => "circle",
            Self::Arc { .. } => "arc",
            Self::Polygon { .. } => "polygon",
            Self::Text { .. } => "text",
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
        }
    }
}

/// Reads a document from its JSON and validates it: the version, the
/// limits, every number finite and in range, every reference and name
/// well formed. What is refused says where and why ([`IntentError`]).
pub fn parse(text: &str) -> Result<Intent, IntentError> {
    let intent: Intent = serde_json::from_str(text).map_err(|error| IntentError::Json {
        line: error.line(),
        column: error.column(),
        message: error.to_string(),
    })?;
    validate::validate(&intent)?;
    Ok(intent)
}

/// The JSON Schema of [`Intent`], for a model's structured output
/// (`features = ["schema"]`).
#[cfg(feature = "schema")]
pub fn schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Intent)).expect("a schema is JSON")
}

pub(crate) fn point(p: Point) -> Point2 {
    Point2::new(p[0], p[1])
}
