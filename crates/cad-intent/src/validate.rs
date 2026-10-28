//! What a document may hold: the limits, and what is refused.

use std::fmt;

use cad_core::tolerance::LinearTolerance;

use crate::{Color, Intent, Measure, Operation, Pick, Point, StairKind, Targets, VERSION, point};

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
    /// Holes one part may have.
    pub const HOLES: u32 = 1000;
    /// Rooms in a floor plan's brief (ADR-0039).
    pub const ROOMS: usize = 24;
    /// Groups of members of one width (#329).
    pub const MEMBER_GROUPS: usize = 16;
    /// The plans a floor plan may choose among.
    pub const ALTERNATIVES: u32 = 3;
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
    /// A field another one needs was not given: a hole's offset with its
    /// diameter.
    Missing,
    /// A field given with another it excludes: a radius with a third
    /// tangent.
    Both(&'static str),
    /// A field the operation does not take in the form asked: a second
    /// object for a fix.
    Unwanted,
    /// A name given twice where each must be one of its own.
    Repeated,
    /// A name that is none of those given: a room not in the brief.
    Unknown,
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
            Self::Missing => write!(f, "must be given"),
            Self::Both(other) => write!(f, "cannot be given with {other}"),
            Self::Unwanted => write!(f, "is not taken here"),
            Self::Repeated => write!(f, "is given twice"),
            Self::Unknown => write!(f, "names what is not there"),
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
    if !crate::READS.contains(&intent.version) {
        return Err(IntentError::Version {
            found: intent.version,
            expected: VERSION,
        });
    }
    // A view's operations count with the rest.
    let count: usize = intent
        .operations
        .iter()
        .map(|operation| match operation {
            Operation::View { operations, .. } => 1 + operations.len(),
            _ => 1,
        })
        .sum();
    if count > limits::OPERATIONS {
        return Err(IntentError::TooManyOperations {
            count,
            limit: limits::OPERATIONS,
        });
    }
    let mut views: Vec<&str> = Vec::new();
    for (index, operation) in intent.operations.iter().enumerate() {
        let check = Check {
            operation: index,
            op: operation.name(),
        };
        check.operation(operation)?;
        if let Operation::View { id, operations, .. } = operation {
            if views.contains(&id.as_str()) {
                return Err(check.refuse("id", Reason::Repeated));
            }
            views.push(id);
            for inner in operations {
                if matches!(inner, Operation::View { .. }) {
                    return Err(check.refuse("operations", Reason::Unwanted));
                }
                Check {
                    operation: index,
                    op: inner.name(),
                }
                .operation(inner)?;
            }
        }
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

    /// A count of holes, rows, or columns: one or more, within the limit.
    fn count(&self, field: &'static str, count: u32) -> Result<(), IntentError> {
        if count == 0 {
            return Err(self.refuse(field, Reason::NotPositive));
        }
        if count > limits::HOLES {
            return Err(self.refuse(field, Reason::TooMany(limits::HOLES as usize)));
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

    /// Points on the isometric axes: each coordinate a number, and two in
    /// a row never on the same place of the page.
    fn iso_points(
        &self,
        field: &'static str,
        points: &[crate::IsoPoint],
        least: usize,
    ) -> Result<(), IntentError> {
        for point in points {
            for value in point {
                self.number(field, *value)?;
            }
        }
        let mapped: Vec<Point> = points.iter().map(|p| crate::iso_point(*p)).collect();
        self.points(field, &mapped, least)
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
            Operation::IsoLine { points } => self.iso_points("points", points, 2),
            Operation::IsoPolyline { points, closed } => {
                self.iso_points("points", points, if *closed { 3 } else { 2 })
            }
            Operation::IsoCircle { center, radius, .. } => {
                self.iso_points("center", &[*center], 1)?;
                self.positive("radius", *radius)
            }
            Operation::Polyline { points, closed } => {
                self.points("points", points, if *closed { 3 } else { 2 })
            }
            Operation::Spline { points } => self.points("points", points, 2),
            Operation::View {
                id,
                title,
                placed,
                of,
                looking,
                ratio,
                cut,
                operations,
            } => {
                if let Some(ratio) = ratio {
                    if !matches!(
                        placed,
                        crate::ViewPlacement::Free | crate::ViewPlacement::Section
                    ) {
                        return Err(self.refuse("ratio", Reason::Unwanted));
                    }
                    self.positive("ratio", *ratio)?;
                }
                match (placed, cut) {
                    (crate::ViewPlacement::Section, Some(cut)) => {
                        self.name("cut.of", &cut.of)?;
                        self.point("cut.from", cut.from)?;
                        self.point("cut.to", cut.to)?;
                        self.point("cut.toward", cut.toward)?;
                        self.distinct("cut.to", cut.from, cut.to)?;
                    }
                    (crate::ViewPlacement::Section, None) => {
                        return Err(self.refuse("cut", Reason::Missing));
                    }
                    (_, Some(_)) => return Err(self.refuse("cut", Reason::Unwanted)),
                    (_, None) => {}
                }
                self.name("id", id)?;
                if !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    || id.len() > 40
                {
                    return Err(self.refuse("id", Reason::BadName));
                }
                if !title.is_empty() {
                    self.text("title", title)?;
                }
                match placed {
                    crate::ViewPlacement::Projected => {
                        match of {
                            Some(of) => self.name("of", of)?,
                            None => return Err(self.refuse("of", Reason::Missing)),
                        }
                        if looking.is_none() {
                            return Err(self.refuse("looking", Reason::Missing));
                        }
                    }
                    _ => {
                        if of.is_some() {
                            return Err(self.refuse("of", Reason::Unwanted));
                        }
                        if looking.is_some() {
                            return Err(self.refuse("looking", Reason::Unwanted));
                        }
                    }
                }
                if operations.is_empty() {
                    return Err(self.refuse("operations", Reason::TooFew(1)));
                }
                Ok(())
            }
            Operation::Hatch {
                inside,
                targets,
                pattern,
                angle,
                scale,
            } => {
                match (inside, targets) {
                    (Some(inside), None) => self.point("inside", *inside)?,
                    (None, Some(targets)) => self.targets("targets", targets)?,
                    (Some(_), Some(_)) => {
                        return Err(self.refuse("inside", Reason::Both("targets")));
                    }
                    (None, None) => return Err(self.refuse("inside", Reason::Missing)),
                }
                self.name("pattern", pattern)?;
                self.number("angle", *angle)?;
                self.positive("scale", *scale)
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
            Operation::Erase { targets } => self.targets("targets", targets),
            Operation::Members { groups, .. } => {
                if groups.is_empty() {
                    return Err(self.refuse("groups", Reason::TooFew(1)));
                }
                if groups.len() > limits::MEMBER_GROUPS {
                    return Err(self.refuse("groups", Reason::TooMany(limits::MEMBER_GROUPS)));
                }
                for group in groups {
                    self.targets("groups.targets", &group.targets)?;
                    self.positive("groups.width", group.width)?;
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
            Operation::DimScale { scale } => self.positive("scale", *scale),
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
            Operation::Plate {
                corner,
                width,
                height,
                fillet,
                chamfer,
                hole_diameter,
                hole_offset,
            } => {
                self.point("corner", *corner)?;
                self.positive("width", *width)?;
                self.positive("height", *height)?;
                if let Some(r) = fillet {
                    self.positive("fillet", *r)?;
                }
                if let Some(d) = chamfer {
                    self.positive("chamfer", *d)?;
                    if fillet.is_some() {
                        return Err(self.refuse("chamfer", Reason::TooMany(1)));
                    }
                }
                match (hole_diameter, hole_offset) {
                    (Some(d), Some(o)) => {
                        self.positive("hole_diameter", *d)?;
                        self.positive("hole_offset", *o)
                    }
                    (None, None) => Ok(()),
                    (Some(_), None) => Err(self.refuse("hole_offset", Reason::Missing)),
                    (None, Some(_)) => Err(self.refuse("hole_diameter", Reason::Missing)),
                }
            }
            Operation::Slot {
                center,
                length,
                width,
                angle,
            } => {
                self.point("center", *center)?;
                self.positive("length", *length)?;
                self.positive("width", *width)?;
                self.number("angle", *angle)
            }
            Operation::HolePattern {
                first,
                diameter,
                rows,
                columns,
                row_spacing,
                column_spacing,
            } => {
                self.point("first", *first)?;
                self.positive("diameter", *diameter)?;
                self.count("rows", *rows)?;
                self.count("columns", *columns)?;
                if rows.saturating_mul(*columns) > limits::HOLES {
                    return Err(self.refuse("columns", Reason::TooMany(limits::HOLES as usize)));
                }
                self.number("row_spacing", *row_spacing)?;
                self.number("column_spacing", *column_spacing)
            }
            Operation::BoltCircle {
                center,
                pitch_diameter,
                count,
                hole_diameter,
                start_angle,
            } => {
                self.point("center", *center)?;
                self.positive("pitch_diameter", *pitch_diameter)?;
                self.count("count", *count)?;
                self.positive("hole_diameter", *hole_diameter)?;
                self.number("start_angle", *start_angle)
            }
            Operation::RoundFlange {
                center,
                outer_diameter,
                bore_diameter,
                pitch_diameter,
                count,
                hole_diameter,
            } => {
                self.point("center", *center)?;
                self.positive("outer_diameter", *outer_diameter)?;
                self.positive("bore_diameter", *bore_diameter)?;
                self.positive("pitch_diameter", *pitch_diameter)?;
                self.count("count", *count)?;
                self.positive("hole_diameter", *hole_diameter)
            }
            Operation::RectangularFlange {
                center,
                width,
                height,
                bore_diameter,
                hole_diameter,
                hole_offset,
            } => {
                self.point("center", *center)?;
                self.positive("width", *width)?;
                self.positive("height", *height)?;
                self.positive("bore_diameter", *bore_diameter)?;
                self.positive("hole_diameter", *hole_diameter)?;
                self.positive("hole_offset", *hole_offset)
            }
            Operation::TangentCircle {
                first,
                second,
                third,
                radius,
            } => {
                self.pick("first", first)?;
                self.pick("second", second)?;
                match (third, radius) {
                    (Some(third), None) => self.pick("third", third),
                    (None, Some(radius)) => self.positive("radius", *radius),
                    (Some(_), Some(_)) => Err(self.refuse("radius", Reason::Both("third"))),
                    (None, None) => Err(self.refuse("radius", Reason::Missing)),
                }
            }
            Operation::TangentArc {
                first,
                second,
                radius,
            } => {
                self.pick("first", first)?;
                self.pick("second", second)?;
                self.positive("radius", *radius)
            }
            Operation::ConcentricCircle { of, radius } => {
                self.pick("of", of)?;
                self.positive("radius", *radius)
            }
            Operation::PathArray {
                targets,
                path,
                count,
                spacing,
            } => {
                self.targets("targets", targets)?;
                self.pick("path", path)?;
                match (count, spacing) {
                    (Some(count), None) => {
                        if *count < 2 {
                            return Err(self.refuse("count", Reason::TooFew(2)));
                        }
                        if *count > limits::COPIES {
                            return Err(
                                self.refuse("count", Reason::TooMany(limits::COPIES as usize))
                            );
                        }
                        Ok(())
                    }
                    (None, Some(spacing)) => self.positive("spacing", *spacing),
                    (Some(_), Some(_)) => Err(self.refuse("spacing", Reason::Both("count"))),
                    (None, None) => Err(self.refuse("count", Reason::Missing)),
                }
            }
            Operation::GridArray {
                targets,
                inside,
                column_spacing,
                row_spacing,
            } => {
                self.targets("targets", targets)?;
                self.point("inside", *inside)?;
                self.positive("column_spacing", *column_spacing)?;
                if let Some(row_spacing) = row_spacing {
                    self.positive("row_spacing", *row_spacing)?;
                }
                Ok(())
            }
            Operation::Constrain {
                relation,
                first,
                second,
            } => {
                self.pick("first", first)?;
                match (relation.takes_one(), second) {
                    (true, Some(_)) => Err(self.refuse("second", Reason::Unwanted)),
                    (false, None) => Err(self.refuse("second", Reason::Missing)),
                    (true, None) => Ok(()),
                    (false, Some(second)) => self.pick("second", second),
                }
            }
            Operation::ConstrainDimension {
                measure,
                points,
                of,
                and,
                at,
                value,
            } => {
                if let Some([a, b]) = points {
                    self.distinct("points", *a, *b)?;
                }
                if let Some(of) = of {
                    self.pick("of", of)?;
                }
                if let Some(and) = and {
                    self.pick("and", and)?;
                }
                match measure {
                    m if m.is_linear() => match (points, of) {
                        (Some(_), Some(_)) => return Err(self.refuse("of", Reason::Both("points"))),
                        (None, None) => return Err(self.refuse("points", Reason::Missing)),
                        _ if and.is_some() => return Err(self.refuse("and", Reason::Unwanted)),
                        _ => {}
                    },
                    Measure::Angular => {
                        if points.is_some() {
                            return Err(self.refuse("points", Reason::Unwanted));
                        }
                        if of.is_none() {
                            return Err(self.refuse("of", Reason::Missing));
                        }
                        if and.is_none() {
                            return Err(self.refuse("and", Reason::Missing));
                        }
                    }
                    _ => {
                        if points.is_some() {
                            return Err(self.refuse("points", Reason::Unwanted));
                        }
                        if and.is_some() {
                            return Err(self.refuse("and", Reason::Unwanted));
                        }
                        if of.is_none() {
                            return Err(self.refuse("of", Reason::Missing));
                        }
                    }
                }
                self.point("at", *at)?;
                if let Some(value) = value {
                    self.positive("value", *value)?;
                }
                Ok(())
            }
            Operation::SetParameter { name, value } => {
                self.name("name", name)?;
                self.positive("value", *value)
            }
            Operation::TitleBlock {
                scale,
                title,
                number,
                drawn_by,
                date,
                sheet,
                at,
                ..
            } => {
                if let Some(scale) = scale {
                    self.positive("scale", *scale)?;
                }
                for (field, value) in [
                    ("title", title),
                    ("number", number),
                    ("drawn_by", drawn_by),
                    ("date", date),
                    ("sheet", sheet),
                ] {
                    if value.chars().count() > limits::TEXT {
                        return Err(self.refuse(field, Reason::TooLong(limits::TEXT)));
                    }
                }
                if let Some(at) = at {
                    self.point("at", *at)?;
                }
                Ok(())
            }
            Operation::PartViews { part, at, .. } => {
                match (&part.profile, &part.steps) {
                    (Some(profile), None) => {
                        self.points("part.profile", profile, 3)?;
                        self.positive("part.depth", part.depth.unwrap_or(0.0))?;
                        if part.bore.is_some() {
                            return Err(self.refuse("part.bore", Reason::Unwanted));
                        }
                    }
                    (None, Some(steps)) => {
                        if steps.is_empty() {
                            return Err(self.refuse("part.steps", Reason::TooFew(1)));
                        }
                        for [diameter, length] in steps {
                            self.positive("part.steps", *diameter)?;
                            self.positive("part.steps", *length)?;
                        }
                        if let Some(bore) = part.bore {
                            self.not_negative("part.bore", bore)?;
                        }
                        if part.depth.is_some() || !part.holes.is_empty() {
                            return Err(self.refuse("part.depth", Reason::Unwanted));
                        }
                    }
                    (Some(_), Some(_)) => {
                        return Err(self.refuse("part.steps", Reason::Both("profile")));
                    }
                    (None, None) => return Err(self.refuse("part", Reason::Nothing)),
                }
                if part.holes.len() > limits::HOLES as usize {
                    return Err(self.refuse("part.holes", Reason::TooMany(limits::HOLES as usize)));
                }
                for hole in &part.holes {
                    self.point("part.holes.at", hole.at)?;
                    self.positive("part.holes.diameter", hole.diameter)?;
                }
                if let Some(at) = at {
                    self.point("at", *at)?;
                }
                Ok(())
            }
            Operation::BuildingSection { plan, from, to, at } => {
                self.name("plan", plan)?;
                self.distinct("to", *from, *to)?;
                if let Some(at) = at {
                    self.point("at", *at)?;
                }
                Ok(())
            }
            Operation::Elevation {
                plan,
                roof_pitch,
                at,
                ..
            } => {
                self.name("plan", plan)?;
                if let Some(pitch) = roof_pitch {
                    self.positive("roof_pitch", *pitch)?;
                }
                if let Some(at) = at {
                    self.point("at", *at)?;
                }
                Ok(())
            }
            Operation::Stair {
                height,
                width,
                kind,
                newel,
                at,
                direction,
                ..
            } => {
                self.positive("height", *height)?;
                self.positive("width", *width)?;
                if let Some(radius) = newel {
                    if *kind != StairKind::Helical {
                        return Err(self.refuse("newel", Reason::Unwanted));
                    }
                    self.positive("newel", *radius)?;
                }
                self.point("at", *at)?;
                if let Some(angle) = direction {
                    self.number("direction", *angle)?;
                }
                Ok(())
            }
            Operation::FloorPlan {
                brief,
                at,
                alternative,
            } => {
                self.positive("brief.terrain.width", brief.terrain.width)?;
                self.positive("brief.terrain.depth", brief.terrain.depth)?;
                if let Some(s) = brief.setbacks {
                    for (field, value) in [
                        ("brief.setbacks.front", s.front),
                        ("brief.setbacks.back", s.back),
                        ("brief.setbacks.left", s.left),
                        ("brief.setbacks.right", s.right),
                    ] {
                        self.not_negative(field, value)?;
                    }
                }
                if brief.rooms.is_empty() {
                    return Err(self.refuse("brief.rooms", Reason::TooFew(1)));
                }
                if brief.rooms.len() > limits::ROOMS {
                    return Err(self.refuse("brief.rooms", Reason::TooMany(limits::ROOMS)));
                }
                for (i, room) in brief.rooms.iter().enumerate() {
                    self.name("brief.rooms.name", &room.name)?;
                    if brief.rooms[..i].iter().any(|r| r.name == room.name) {
                        return Err(self.refuse("brief.rooms.name", Reason::Repeated));
                    }
                    self.positive("brief.rooms.area", room.area)?;
                    if let Some(width) = room.min_width {
                        self.positive("brief.rooms.min_width", width)?;
                    }
                    if let Some(aspect) = room.max_aspect {
                        self.positive("brief.rooms.max_aspect", aspect)?;
                    }
                }
                for pair in &brief.adjacent {
                    if pair[0] == pair[1]
                        || pair
                            .iter()
                            .any(|n| brief.rooms.iter().all(|r| &r.name != n))
                    {
                        return Err(self.refuse("brief.adjacent", Reason::Unknown));
                    }
                }
                if let Some(share) = brief.max_circulation {
                    self.not_negative("brief.max_circulation", share)?;
                }
                if let Some(width) = brief.windows {
                    self.positive("brief.windows", width)?;
                }
                if let Some(height) = brief.storey_height {
                    self.positive("brief.storey_height", height)?;
                }
                if let Some(b) = brief.building {
                    for (field, value) in [
                        ("brief.building.window_head", b.window_head),
                        ("brief.building.door_head", b.door_head),
                        ("brief.building.slab", b.slab),
                        ("brief.building.roof_pitch", b.roof_pitch),
                    ] {
                        if let Some(value) = value {
                            self.positive(field, value)?;
                        }
                    }
                    for (field, value) in [
                        ("brief.building.sill", b.sill),
                        ("brief.building.eaves", b.eaves),
                        ("brief.building.parapet", b.parapet),
                    ] {
                        if let Some(value) = value {
                            self.not_negative(field, value)?;
                        }
                    }
                }
                if let Some(at) = at {
                    self.point("at", *at)?;
                }
                if let Some(n) = alternative
                    && !(1..=limits::ALTERNATIVES).contains(n)
                {
                    return Err(self.refuse(
                        "alternative",
                        Reason::TooMany(limits::ALTERNATIVES as usize),
                    ));
                }
                Ok(())
            }
        }
    }
}
