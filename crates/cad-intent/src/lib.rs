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
pub const VERSION: u32 = 2;

/// The versions this crate reads: each a subset of the next, so a
/// document of an earlier one is read as it was meant. Version 2 added
/// the parts (#272).
pub const READS: [u32; 2] = [1, 2];

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
