//! What a document may hold: the limits, and what is refused.

use std::fmt;

use cad_core::tolerance::LinearTolerance;

use crate::{Color, Intent, Operation, Pick, Point, Targets, VERSION, point};

/// Two points of a document are the same point when the drawing would
/// treat them so (DESIGN §21).
pub(crate) fn same_point(a: Point, b: Point) -> bool {
    LinearTolerance::DEFAULT.coincident(point(a), point(b))
}

/// The bounds a document is held to.
pub mod limits {
    /// Operations per document.
    pub const OPERATIONS: usize = 200;
    /// Points in one line, polyline, or pick list.
    pub const POINTS: usize = 10_000;
    /// Object references in one target list.
    pub const REFERENCES: usize = 10_000;
    /// The magnitude a coordinate or length may reach.
    pub const COORDINATE: f64 = 1.0e9;
    /// Characters in a text.
    pub const TEXT: usize = 1024;
    /// Characters in a name.
    pub const NAME: usize = 255;
    /// Sides of a polygon.
    pub const SIDES: u32 = 1024;
    /// Copies an array may make.
    pub const COPIES: u32 = 10_000;
}

/// Why a value is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A number is NaN or infinite.
    NotFinite,
    /// A coordinate or length is past `limits::COORDINATE`.
    OutOfRange,
    /// A length, radius, factor, or count must be greater than zero.
    NotPositive,
    /// Too few of something: the least allowed.
    TooFew(usize),
    /// Too many of something: the most allowed.
    TooMany(usize),
    /// A name or text is empty, or blank.
    Empty,
    /// A text or name is longer than allowed.
    TooLong(usize),
    /// Two points that must differ are the same.
    Coincident,
    /// A name holds a character a name cannot: a control character, or
    /// one of `<>/\":;?*|=\``.
    BadName,
    /// Nothing was asked for.
    Nothing,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => write!(f, "is not a finite number"),
            Self::OutOfRange => write!(f, "is beyond {:e}", limits::COORDINATE),
            Self::NotPositive => write!(f, "must be greater than zero"),
            Self::TooFew(n) => write!(f, "needs at least {n}"),
            Self::TooMany(n) => write!(f, "takes at most {n}"),
            Self::Empty => write!(f, "is empty"),
            Self::TooLong(n) => write!(f, "is longer than {n} characters"),
            Self::Coincident => write!(f, "repeats a point that must differ"),
            Self::BadName => write!(f, "holds a character a name cannot"),
            Self::Nothing => write!(f, "asks for nothing"),
        }
    }
}

/// What is wrong with a document: where, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentError {
    /// The text is not the JSON of a document.
    Json {
        line: usize,
        column: usize,
        message: String,
    },
    /// A version this crate does not read.
    Version { found: u32, expected: u32 },
    /// More operations than `limits::OPERATIONS`.
    TooManyOperations { count: usize, limit: usize },
    /// An operation's field, refused.
    Invalid {
        /// The operation's index in the document.
        operation: usize,
        /// Its `op` name.
        op: &'static str,
        /// The field, as named in the document (`points`, `radius`, …).
        field: &'static str,
        reason: Reason,
    },
}

impl fmt::Display for IntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json {
                line,
                column,
                message,
            } => write!(
                f,
                "not an intent document (line {line}, column {column}): {message}"
            ),
            Self::Version { found, expected } => {
                write!(f, "version {found} is not read; this is version {expected}")
            }
            Self::TooManyOperations { count, limit } => {
                write!(f, "{count} operations; a document takes at most {limit}")
            }
            Self::Invalid {
                operation,
                op,
                field,
                reason,
            } => write!(f, "operation {operation} ({op}): {field} {reason}"),
        }
    }
}

impl std::error::Error for IntentError {}

pub(crate) fn validate(intent: &Intent) -> Result<(), IntentError> {
    if intent.version != VERSION {
        return Err(IntentError::Version {
            found: intent.version,
            expected: VERSION,
        });
    }
    if intent.operations.len() > limits::OPERATIONS {
        return Err(IntentError::TooManyOperations {
            count: intent.operations.len(),
            limit: limits::OPERATIONS,
        });
    }
    for (index, operation) in intent.operations.iter().enumerate() {
        let check = Check {
            operation: index,
            op: operation.name(),
        };
        check.operation(operation)?;
    }
    Ok(())
}

/// The checks, each naming the field it refuses.
struct Check {
    operation: usize,
    op: &'static str,
}

impl Check {
    fn refuse(&self, field: &'static str, reason: Reason) -> IntentError {
        IntentError::Invalid {
            operation: self.operation,
            op: self.op,
            field,
            reason,
        }
    }

    fn number(&self, field: &'static str, value: f64) -> Result<(), IntentError> {
        if !value.is_finite() {
            return Err(self.refuse(field, Reason::NotFinite));
        }
        if value.abs() > limits::COORDINATE {
            return Err(self.refuse(field, Reason::OutOfRange));
        }
        Ok(())
    }

    fn positive(&self, field: &'static str, value: f64) -> Result<(), IntentError> {
        self.number(field, value)?;
        if value <= 0.0 {
            return Err(self.refuse(field, Reason::NotPositive));
        }
        Ok(())
    }

    fn not_negative(&self, field: &'static str, value: f64) -> Result<(), IntentError> {
        self.number(field, value)?;
        if value < 0.0 {
            return Err(self.refuse(field, Reason::NotPositive));
        }
        Ok(())
    }

    fn point(&self, field: &'static str, point: Point) -> Result<(), IntentError> {
        self.number(field, point[0])?;
        self.number(field, point[1])
    }

    fn points(
        &self,
        field: &'static str,
        points: &[Point],
        least: usize,
    ) -> Result<(), IntentError> {
        if points.len() < least {
            return Err(self.refuse(field, Reason::TooFew(least)));
        }
        if points.len() > limits::POINTS {
            return Err(self.refuse(field, Reason::TooMany(limits::POINTS)));
        }
        for point in points {
            self.point(field, *point)?;
        }
        if points.windows(2).any(|pair| same_point(pair[0], pair[1])) {
            return Err(self.refuse(field, Reason::Coincident));
        }
        Ok(())
    }

    fn distinct(&self, field: &'static str, a: Point, b: Point) -> Result<(), IntentError> {
        self.point(field, a)?;
        self.point(field, b)?;
        if same_point(a, b) {
            return Err(self.refuse(field, Reason::Coincident));
        }
        Ok(())
    }

    fn targets(&self, field: &'static str, targets: &Targets) -> Result<(), IntentError> {
        match targets {
            Targets::Selection(_) => Ok(()),
            Targets::Ids(ids) if ids.is_empty() => Err(self.refuse(field, Reason::TooFew(1))),
            Targets::Ids(ids) if ids.len() > limits::REFERENCES => {
                Err(self.refuse(field, Reason::TooMany(limits::REFERENCES)))
            }
            Targets::Ids(_) => Ok(()),
        }
    }

    fn pick(&self, field: &'static str, pick: &Pick) -> Result<(), IntentError> {
        match pick.near {
            Some(near) => self.point(field, near),
            None => Ok(()),
        }
    }

    fn picks(&self, field: &'static str, picks: &[Pick]) -> Result<(), IntentError> {
        if picks.is_empty() {
            return Err(self.refuse(field, Reason::TooFew(1)));
        }
        if picks.len() > limits::POINTS {
            return Err(self.refuse(field, Reason::TooMany(limits::POINTS)));
        }
        picks.iter().try_for_each(|pick| self.pick(field, pick))
    }

    fn name(&self, field: &'static str, name: &str) -> Result<(), IntentError> {
        if name.trim().is_empty() {
            return Err(self.refuse(field, Reason::Empty));
        }
        if name.chars().count() > limits::NAME {
            return Err(self.refuse(field, Reason::TooLong(limits::NAME)));
        }
        if name
            .chars()
            .any(|c| c.is_control() || "<>/\\\":;?*|=`".contains(c))
        {
            return Err(self.refuse(field, Reason::BadName));
        }
        Ok(())
    }

    fn text(&self, field: &'static str, text: &str) -> Result<(), IntentError> {
        if text.trim().is_empty() {
            return Err(self.refuse(field, Reason::Empty));
        }
        if text.chars().count() > limits::TEXT {
            return Err(self.refuse(field, Reason::TooLong(limits::TEXT)));
        }
        if text.chars().any(char::is_control) {
            return Err(self.refuse(field, Reason::BadName));
        }
        Ok(())
    }

    fn operation(&self, operation: &Operation) -> Result<(), IntentError> {
        match operation {
            Operation::Line { points } => self.points("points", points, 2),
            Operation::Polyline { points, closed } => {
                self.points("points", points, if *closed { 3 } else { 2 })
            }
            Operation::Rectangle { corner, opposite } => {
                self.distinct("opposite", *corner, *opposite)?;
                let flat = LinearTolerance::DEFAULT;
                if flat.is_zero((corner[0] - opposite[0]).abs())
                    || flat.is_zero((corner[1] - opposite[1]).abs())
                {
                    return Err(self.refuse("opposite", Reason::Coincident));
                }
                Ok(())
            }
            Operation::Circle { center, radius } => {
                self.point("center", *center)?;
                self.positive("radius", *radius)
            }
            Operation::Arc {
                start,
                through,
                end,
            } => {
                self.distinct("through", *start, *through)?;
                self.distinct("end", *through, *end)?;
                self.distinct("end", *start, *end)
            }
            Operation::Polygon {
                sides,
                center,
                radius,
                ..
            } => {
                if *sides < 3 {
                    return Err(self.refuse("sides", Reason::TooFew(3)));
                }
                if *sides > limits::SIDES {
                    return Err(self.refuse("sides", Reason::TooMany(limits::SIDES as usize)));
                }
                self.point("center", *center)?;
                self.positive("radius", *radius)
            }
            Operation::Text {
                at,
                text,
                height,
                rotation,
            } => {
                self.point("at", *at)?;
                self.text("text", text)?;
                if let Some(height) = height {
                    self.positive("height", *height)?;
                }
                if let Some(rotation) = rotation {
                    self.number("rotation", *rotation)?;
                }
                Ok(())
            }
            Operation::Move {
                targets,
                displacement,
            }
            | Operation::Copy {
                targets,
                displacement,
            } => {
                self.targets("targets", targets)?;
                self.point("displacement", *displacement)
            }
            Operation::Rotate {
                targets,
                base,
                angle,
            } => {
                self.targets("targets", targets)?;
                self.point("base", *base)?;
                self.number("angle", *angle)
            }
            Operation::Scale {
                targets,
                base,
                factor,
            } => {
                self.targets("targets", targets)?;
                self.point("base", *base)?;
                self.positive("factor", *factor)
            }
            Operation::Mirror { targets, axis, .. } => {
                self.targets("targets", targets)?;
                self.distinct("axis", axis[0], axis[1])
            }
            Operation::Offset {
                target,
                distance,
                toward,
            } => {
                self.pick("target", target)?;
                self.positive("distance", *distance)?;
                self.point("toward", *toward)
            }
            Operation::Trim { edges, targets } | Operation::Extend { edges, targets } => {
                if let Some(edges) = edges {
                    self.targets("edges", edges)?;
                }
                self.picks("targets", targets)
            }
            Operation::Fillet {
                radius,
                first,
                second,
            } => {
                self.not_negative("radius", *radius)?;
                self.pick("first", first)?;
                self.pick("second", second)
            }
            Operation::Chamfer {
                first_distance,
                second_distance,
                first,
                second,
            } => {
                self.not_negative("first_distance", *first_distance)?;
                self.not_negative("second_distance", *second_distance)?;
                self.pick("first", first)?;
                self.pick("second", second)
            }
            Operation::Array {
                targets,
                rows,
                columns,
                row_spacing,
                column_spacing,
            } => {
                self.targets("targets", targets)?;
                if *rows == 0 {
                    return Err(self.refuse("rows", Reason::TooFew(1)));
                }
                if *columns == 0 {
                    return Err(self.refuse("columns", Reason::TooFew(1)));
                }
                if u64::from(*rows) * u64::from(*columns) > u64::from(limits::COPIES) {
                    return Err(self.refuse("columns", Reason::TooMany(limits::COPIES as usize)));
                }
                if *rows * *columns == 1 {
                    return Err(self.refuse("rows", Reason::Nothing));
                }
                self.number("row_spacing", *row_spacing)?;
                self.number("column_spacing", *column_spacing)
            }
            Operation::PolarArray {
                targets,
                center,
                count,
                angle,
                ..
            } => {
                self.targets("targets", targets)?;
                self.point("center", *center)?;
                if *count < 2 {
                    return Err(self.refuse("count", Reason::TooFew(2)));
                }
                if *count > limits::COPIES {
                    return Err(self.refuse("count", Reason::TooMany(limits::COPIES as usize)));
                }
                self.number("angle", *angle)?;
                if *angle == 0.0 {
                    return Err(self.refuse("angle", Reason::NotPositive));
                }
                Ok(())
            }
            Operation::Layer { name, .. } => self.name("name", name),
            Operation::SetProperties {
                targets,
                layer,
                color,
                linetype,
                lineweight,
            } => {
                self.targets("targets", targets)?;
                if layer.is_none() && color.is_none() && linetype.is_none() && lineweight.is_none()
                {
                    return Err(self.refuse("layer", Reason::Nothing));
                }
                if let Some(layer) = layer {
                    self.name("layer", layer)?;
                }
                match color {
                    Some(Color::Index(index)) if !(1..=255).contains(index) => {
                        return Err(self.refuse("color", Reason::OutOfRange));
                    }
                    Some(Color::Name(name)) => self.name("color", name)?,
                    _ => {}
                }
                if let Some(linetype) = linetype {
                    self.name("linetype", linetype)?;
                }
                if let Some(lineweight) = lineweight {
                    self.name("lineweight", lineweight)?;
                }
                Ok(())
            }
            Operation::DimLinear { from, to, at, .. } | Operation::DimAligned { from, to, at } => {
                self.distinct("to", *from, *to)?;
                self.point("at", *at)
            }
            Operation::DimRadius { target, at } | Operation::DimDiameter { target, at } => {
                self.pick("target", target)?;
                self.point("at", *at)
            }
            Operation::DimAngular {
                vertex,
                first,
                second,
                at,
            } => {
                self.distinct("first", *vertex, *first)?;
                self.distinct("second", *vertex, *second)?;
                self.distinct("second", *first, *second)?;
                self.point("at", *at)
            }
        }
    }
}
