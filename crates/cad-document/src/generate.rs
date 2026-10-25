//! Part generators (#272, ADR-0038): plates, slots, hole patterns, bolt
//! circles, and flanges, as constrained geometry.
//!
//! A [`Spec`] is checked — an impossible part is refused with the reason —
//! and becomes [`Shapes`]: the entities, and the constraints between them
//! as templates over their indices, so the same shapes serve a preview
//! (the entities alone) and the part placed in a transaction (entities and
//! constraints, as one undo). The constraints hold what a drafter would
//! dimension: sizes as dimensional constraints, named as DIMCONSTRAINT
//! names them, and the rest geometric, so the part stays itself when one
//! value changes.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use cad_core::{EntityId, Point2, Vec2};

use crate::constraint::{Axis, Constraint, Kind, Part, Reference, Touch};
use crate::drawing::{TransactionBuilder, TransactionError};
use crate::{ArcEntity, CircleEntity, Entity, LineEntity};

/// A plate's corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Corners {
    Square,
    /// Rounded, to this radius.
    Fillet(f64),
    /// Bevelled, this far along each side.
    Chamfer(f64),
}

/// Holes at a plate's corners: their diameter, and how far their centres
/// are in from both edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerHoles {
    pub diameter: f64,
    pub offset: f64,
}

/// A part to generate.
#[derive(Clone, Debug, PartialEq)]
pub enum Spec {
    /// A rectangular plate from its lower left corner.
    Plate {
        corner: Point2,
        width: f64,
        height: f64,
        corners: Corners,
        holes: Option<CornerHoles>,
    },
    /// A slot: two half circles joined by two lines, its centres `length`
    /// apart along `angle` (radians), `width` across.
    Slot {
        center: Point2,
        length: f64,
        width: f64,
        angle: f64,
    },
    /// Holes in rows and columns from the first, lower left one.
    HolePattern {
        first: Point2,
        diameter: f64,
        rows: u32,
        columns: u32,
        row_spacing: f64,
        column_spacing: f64,
    },
    /// Holes equally spaced on a pitch circle, the first at `start_angle`.
    BoltCircle {
        center: Point2,
        pitch_diameter: f64,
        count: u32,
        hole_diameter: f64,
        start_angle: f64,
    },
    /// A round flange: an outer circle, a bore, and a bolt circle.
    RoundFlange {
        center: Point2,
        outer_diameter: f64,
        bore_diameter: f64,
        pitch_diameter: f64,
        count: u32,
        hole_diameter: f64,
    },
    /// A rectangular flange: a plate about its centre, a bore, and a hole
    /// at each corner.
    RectangularFlange {
        center: Point2,
        width: f64,
        height: f64,
        bore_diameter: f64,
        hole_diameter: f64,
        offset: f64,
    },
}

/// Why a part cannot be made, as the command says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerateError(pub String);

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn refuse<T>(why: impl Into<String>) -> Result<T, GenerateError> {
    Err(GenerateError(why.into()))
}

/// The most holes one pattern makes.
pub const MAX_HOLES: u32 = 1000;

/// A constraint between the shapes' entities, by index.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub kind: Kind,
    pub references: Vec<(usize, Part)>,
    /// The name's prefix, `d` or `dia`, for a dimensional constraint.
    pub prefix: Option<&'static str>,
    pub place: Option<Point2>,
}

/// A part's entities, and the constraints between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shapes {
    pub entities: Vec<Entity>,
    pub templates: Vec<Template>,
}

impl Shapes {
    fn add(&mut self, entity: impl Into<Entity>) -> usize {
        self.entities.push(entity.into());
        self.entities.len() - 1
    }

    fn line(&mut self, a: Point2, b: Point2) -> usize {
        self.add(LineEntity::new(a, b))
    }

    fn circle(&mut self, center: Point2, radius: f64) -> usize {
        self.add(CircleEntity::new(center, radius))
    }

    fn arc(&mut self, center: Point2, radius: f64, start: f64, end: f64) -> usize {
        self.add(ArcEntity::new(center, radius, start, end))
    }

    fn geometric(&mut self, kind: Kind, references: Vec<(usize, Part)>) {
        self.templates.push(Template {
            kind,
            references,
            prefix: None,
            place: None,
        });
    }

    fn dimension(
        &mut self,
        prefix: &'static str,
        kind: Kind,
        references: Vec<(usize, Part)>,
        place: Point2,
    ) {
        self.templates.push(Template {
            kind,
            references,
            prefix: Some(prefix),
            place: Some(place),
        });
    }

    fn coincident(&mut self, a: (usize, Part), b: (usize, Part)) {
        self.geometric(Kind::Coincident, vec![a, b]);
    }

    /// `to`'s x less `from`'s: a horizontal dimension.
    fn across(&mut self, from: (usize, Part), to: (usize, Part), value: f64, place: Point2) {
        self.dimension(
            "d",
            Kind::Distance {
                axis: Axis::Horizontal,
                value,
            },
            vec![from, to],
            place,
        );
    }

    /// `to`'s y less `from`'s: a vertical dimension.
    fn up(&mut self, from: (usize, Part), to: (usize, Part), value: f64, place: Point2) {
        self.dimension(
            "d",
            Kind::Distance {
                axis: Axis::Vertical,
                value,
            },
            vec![from, to],
            place,
        );
    }

    /// Equal holes: the first's diameter dimensioned, the rest equal to it.
    fn equal_holes(&mut self, holes: &[usize], diameter: f64, place: Point2) {
        let Some((&first, rest)) = holes.split_first() else {
            return;
        };
        self.dimension(
            "dia",
            Kind::Diameter { value: diameter },
            vec![(first, Part::Edge)],
            place,
        );
        for &hole in rest {
            self.geometric(Kind::Equal, vec![(hole, Part::Edge), (first, Part::Edge)]);
        }
    }
}

fn positive(value: f64, what: &str) -> Result<(), GenerateError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        refuse(format!("The {what} must be more than zero."))
    }
}

/// The four sides of a plate, by index in its shapes: bottom, right, top,
/// left, each drawn counter-clockwise round the plate.
struct Sides {
    bottom: usize,
    right: usize,
    top: usize,
    left: usize,
}

/// A plate's outline at `corner`, `width` by `height`, with its corners,
/// constrained: sides level and plumb, joined at their ends (or through
/// the corners' arcs or bevels), the width and height dimensioned.
fn outline(
    shapes: &mut Shapes,
    corner: Point2,
    width: f64,
    height: f64,
    corners: Corners,
) -> Sides {
    let (x, y, w, h) = (corner.x, corner.y, width, height);
    let p = Point2::new;
    let cut = match corners {
        Corners::Square => 0.0,
        Corners::Fillet(r) | Corners::Chamfer(r) => r,
    };
    let bottom = shapes.line(p(x + cut, y), p(x + w - cut, y));
    let right = shapes.line(p(x + w, y + cut), p(x + w, y + h - cut));
    let top = shapes.line(p(x + w - cut, y + h), p(x + cut, y + h));
    let left = shapes.line(p(x, y + h - cut), p(x, y + cut));
    let sides = [bottom, right, top, left];
    match corners {
        Corners::Square => {
            for i in 0..4 {
                shapes.coincident((sides[i], Part::End), (sides[(i + 1) % 4], Part::Start));
            }
        }
        Corners::Fillet(r) => {
            // Each corner's arc, counter-clockwise from the side before it
            // to the side after, tangent to both.
            let centers = [
                p(x + w - r, y + r),
                p(x + w - r, y + h - r),
                p(x + r, y + h - r),
                p(x + r, y + r),
            ];
            let mut arcs = Vec::new();
            for (i, center) in centers.into_iter().enumerate() {
                let start = -FRAC_PI_2 + i as f64 * FRAC_PI_2;
                let arc = shapes.arc(center, r, start, start + FRAC_PI_2);
                let (before, after) = (sides[i], sides[(i + 1) % 4]);
                shapes.coincident((before, Part::End), (arc, Part::Start));
                shapes.coincident((arc, Part::End), (after, Part::Start));
                for side in [before, after] {
                    shapes.geometric(
                        Kind::Tangent { touch: Touch::Left },
                        vec![(side, Part::Edge), (arc, Part::Edge)],
                    );
                }
                arcs.push(arc);
            }
            shapes.dimension(
                "rad",
                Kind::Radius { value: r },
                vec![(arcs[0], Part::Edge)],
                p(x + w + r, y - r),
            );
            for &arc in &arcs[1..] {
                shapes.geometric(Kind::Equal, vec![(arc, Part::Edge), (arcs[0], Part::Edge)]);
            }
        }
        Corners::Chamfer(d) => {
            let ends = [
                (p(x + w - d, y), p(x + w, y + d)),
                (p(x + w, y + h - d), p(x + w - d, y + h)),
                (p(x + d, y + h), p(x, y + h - d)),
                (p(x, y + d), p(x + d, y)),
            ];
            let mut bevels = Vec::new();
            for (i, (a, b)) in ends.into_iter().enumerate() {
                let bevel = shapes.line(a, b);
                shapes.coincident((sides[i], Part::End), (bevel, Part::Start));
                shapes.coincident((bevel, Part::End), (sides[(i + 1) % 4], Part::Start));
                bevels.push(bevel);
            }
            shapes.across(
                (bevels[0], Part::Start),
                (bevels[0], Part::End),
                d,
                p(x + w - d / 2.0, y - d),
            );
            shapes.up(
                (bevels[0], Part::Start),
                (bevels[0], Part::End),
                d,
                p(x + w + d, y + d / 2.0),
            );
            for (i, &bevel) in bevels.iter().enumerate().skip(1) {
                shapes.geometric(
                    Kind::Equal,
                    vec![(bevel, Part::Edge), (bevels[0], Part::Edge)],
                );
                let kind = if i % 2 == 0 {
                    Kind::Parallel
                } else {
                    Kind::Perpendicular
                };
                shapes.geometric(kind, vec![(bevel, Part::Edge), (bevels[0], Part::Edge)]);
            }
        }
    }
    shapes.geometric(Kind::Horizontal, vec![(bottom, Part::Edge)]);
    shapes.geometric(Kind::Horizontal, vec![(top, Part::Edge)]);
    shapes.geometric(Kind::Vertical, vec![(right, Part::Edge)]);
    shapes.geometric(Kind::Vertical, vec![(left, Part::Edge)]);
    let below = y - 0.15 * w.max(h);
    let beside = x - 0.15 * w.max(h);
    shapes.across(
        (left, Part::End),
        (right, Part::Start),
        w,
        p(x + w / 2.0, below),
    );
    shapes.up(
        (bottom, Part::Start),
        (top, Part::End),
        h,
        p(beside, y + h / 2.0),
    );
    Sides {
        bottom,
        right,
        top,
        left,
    }
}

/// A hole at each corner of a plate, `offset` in from both edges: each
/// placed by its distances from the nearest sides.
fn corner_holes(
    shapes: &mut Shapes,
    sides: &Sides,
    corner: Point2,
    w: f64,
    h: f64,
    holes: CornerHoles,
) {
    let (x, y, o) = (corner.x, corner.y, holes.offset);
    let r = holes.diameter / 2.0;
    let p = Point2::new;
    let at = [
        p(x + o, y + o),
        p(x + w - o, y + o),
        p(x + w - o, y + h - o),
        p(x + o, y + h - o),
    ];
    let ids: Vec<usize> = at.iter().map(|&c| shapes.circle(c, r)).collect();
    let hole = |i: usize| (ids[i], Part::Center);
    let note = |c: Point2, dx: f64, dy: f64| p(c.x + dx * o, c.y + dy * o);
    // Lower left, lower right, upper right, upper left.
    shapes.across(
        (sides.left, Part::Start),
        hole(0),
        o,
        note(at[0], -0.5, -1.5),
    );
    shapes.up(
        (sides.bottom, Part::Start),
        hole(0),
        o,
        note(at[0], -1.5, -0.5),
    );
    shapes.across(
        hole(1),
        (sides.right, Part::Start),
        o,
        note(at[1], 0.5, -1.5),
    );
    shapes.up(
        (sides.bottom, Part::Start),
        hole(1),
        o,
        note(at[1], 1.5, -0.5),
    );
    shapes.across(
        hole(2),
        (sides.right, Part::Start),
        o,
        note(at[2], 0.5, 1.5),
    );
    shapes.up(hole(2), (sides.top, Part::Start), o, note(at[2], 1.5, 0.5));
    shapes.across(
        (sides.left, Part::Start),
        hole(3),
        o,
        note(at[3], -0.5, 1.5),
    );
    shapes.up(hole(3), (sides.top, Part::Start), o, note(at[3], -1.5, 0.5));
    shapes.equal_holes(&ids, holes.diameter, note(at[0], 1.5, 1.5));
}

/// Holes equally spaced on a pitch circle: the circle, its diameter
/// dimensioned, the holes on it, each the chord's length from the last.
fn bolts(
    shapes: &mut Shapes,
    center: Point2,
    pitch_diameter: f64,
    count: u32,
    hole_diameter: f64,
    start: f64,
) -> usize {
    let pitch_radius = pitch_diameter / 2.0;
    let pitch = shapes.circle(center, pitch_radius);
    shapes.dimension(
        "dia",
        Kind::Diameter {
            value: pitch_diameter,
        },
        vec![(pitch, Part::Edge)],
        center + Vec2::new(pitch_radius * 0.7, -pitch_radius * 1.2),
    );
    let step = TAU / f64::from(count);
    let chord = pitch_diameter * (step / 2.0).sin();
    let holes: Vec<usize> = (0..count)
        .map(|i| {
            let angle = start + f64::from(i) * step;
            shapes.circle(
                center + Vec2::new(angle.cos(), angle.sin()) * pitch_radius,
                hole_diameter / 2.0,
            )
        })
        .collect();
    for (i, &hole) in holes.iter().enumerate() {
        shapes.coincident((hole, Part::Center), (pitch, Part::Edge));
        if i > 0 {
            shapes.dimension(
                "d",
                Kind::Distance {
                    axis: Axis::Aligned,
                    value: chord,
                },
                vec![(holes[i - 1], Part::Center), (hole, Part::Center)],
                center,
            );
        }
    }
    let first = holes[0];
    let place = match &shapes.entities[first] {
        Entity::Circle(c) => c.center + Vec2::new(hole_diameter, hole_diameter),
        _ => center,
    };
    shapes.equal_holes(&holes, hole_diameter, place);
    pitch
}

impl Spec {
    /// The part's shapes, or why it cannot be made.
    pub fn shapes(&self) -> Result<Shapes, GenerateError> {
        let mut shapes = Shapes::default();
        match *self {
            Spec::Plate {
                corner,
                width,
                height,
                corners,
                holes,
            } => {
                positive(width, "width")?;
                positive(height, "height")?;
                let small = width.min(height);
                match corners {
                    Corners::Fillet(r) => {
                        positive(r, "fillet radius")?;
                        if 2.0 * r >= small {
                            return refuse("The fillet radius is too large for the plate.");
                        }
                    }
                    Corners::Chamfer(d) => {
                        positive(d, "chamfer distance")?;
                        if 2.0 * d >= small {
                            return refuse("The chamfer is too large for the plate.");
                        }
                    }
                    Corners::Square => {}
                }
                if let Some(holes) = holes {
                    positive(holes.diameter, "hole diameter")?;
                    positive(holes.offset, "hole offset")?;
                    let r = holes.diameter / 2.0;
                    let o = holes.offset;
                    if o <= r {
                        return refuse("The holes would cut the edges: move them further in.");
                    }
                    if small - 2.0 * o <= holes.diameter {
                        return refuse(
                            "The holes would run into each other: the plate is too small for them.",
                        );
                    }
                    let clear = match corners {
                        Corners::Fillet(f) if o < f => (o - f).hypot(o - f) + r < f,
                        Corners::Chamfer(d) => (2.0 * o - d) / 2f64.sqrt() > r,
                        _ => true,
                    };
                    if !clear {
                        return refuse("The holes would cut the corners: move them further in.");
                    }
                }
                let sides = outline(&mut shapes, corner, width, height, corners);
                if let Some(holes) = holes {
                    corner_holes(&mut shapes, &sides, corner, width, height, holes);
                }
            }
            Spec::Slot {
                center,
                length,
                width,
                angle,
            } => {
                positive(length, "slot length")?;
                positive(width, "slot width")?;
                let u = Vec2::new(angle.cos(), angle.sin());
                let n = Vec2::new(-u.y, u.x);
                let r = width / 2.0;
                let (p1, p2) = (center - u * (length / 2.0), center + u * (length / 2.0));
                let top = shapes.line(p1 + n * r, p2 + n * r);
                let bottom = shapes.line(p2 - n * r, p1 - n * r);
                let right = shapes.arc(p2, r, angle - FRAC_PI_2, angle + FRAC_PI_2);
                let left = shapes.arc(p1, r, angle + FRAC_PI_2, angle + FRAC_PI_2 + PI);
                shapes.coincident((bottom, Part::Start), (right, Part::Start));
                shapes.coincident((right, Part::End), (top, Part::End));
                shapes.coincident((top, Part::Start), (left, Part::Start));
                shapes.coincident((left, Part::End), (bottom, Part::End));
                for line in [top, bottom] {
                    for arc in [right, left] {
                        shapes.geometric(
                            Kind::Tangent {
                                touch: Touch::Right,
                            },
                            vec![(line, Part::Edge), (arc, Part::Edge)],
                        );
                    }
                }
                shapes.geometric(Kind::Equal, vec![(left, Part::Edge), (right, Part::Edge)]);
                shapes.dimension(
                    "dia",
                    Kind::Diameter { value: width },
                    vec![(right, Part::Edge)],
                    p2 + u * width,
                );
                shapes.dimension(
                    "d",
                    Kind::Distance {
                        axis: Axis::Aligned,
                        value: length,
                    },
                    vec![(left, Part::Center), (right, Part::Center)],
                    center + n * (width * 1.2),
                );
            }
            Spec::HolePattern {
                first,
                diameter,
                rows,
                columns,
                row_spacing,
                column_spacing,
            } => {
                positive(diameter, "hole diameter")?;
                if rows == 0 || columns == 0 {
                    return refuse("A pattern has at least one row and one column.");
                }
                if rows.saturating_mul(columns) > MAX_HOLES {
                    return refuse(format!("A pattern makes at most {MAX_HOLES} holes."));
                }
                if rows > 1 && !(row_spacing.is_finite() && row_spacing.abs() > diameter) {
                    return refuse("The rows would run into each other: space them further apart.");
                }
                if columns > 1 && !(column_spacing.is_finite() && column_spacing.abs() > diameter) {
                    return refuse(
                        "The columns would run into each other: space them further apart.",
                    );
                }
                let mut grid = Vec::new();
                for row in 0..rows {
                    for column in 0..columns {
                        let at = first
                            + Vec2::new(
                                f64::from(column) * column_spacing,
                                f64::from(row) * row_spacing,
                            );
                        grid.push(shapes.circle(at, diameter / 2.0));
                    }
                }
                let cols = columns as usize;
                for (i, &hole) in grid.iter().enumerate() {
                    let (row, column) = (i / cols, i % cols);
                    let center = |k: usize| (grid[k], Part::Center);
                    let at = match &shapes.entities[hole] {
                        Entity::Circle(c) => c.center,
                        _ => first,
                    };
                    if column > 0 {
                        let left = i - 1;
                        shapes.across(
                            center(left),
                            center(i),
                            column_spacing,
                            at + Vec2::new(-column_spacing / 2.0, -diameter),
                        );
                        shapes.geometric(Kind::Horizontal, vec![center(left), center(i)]);
                    } else if row > 0 {
                        let below = i - cols;
                        shapes.up(
                            center(below),
                            center(i),
                            row_spacing,
                            at + Vec2::new(-diameter, -row_spacing / 2.0),
                        );
                        shapes.geometric(Kind::Vertical, vec![center(below), center(i)]);
                    }
                }
                shapes.equal_holes(&grid, diameter, first + Vec2::new(-diameter, -diameter));
            }
            Spec::BoltCircle {
                center,
                pitch_diameter,
                count,
                hole_diameter,
                start_angle,
            } => {
                positive(pitch_diameter, "pitch circle diameter")?;
                positive(hole_diameter, "hole diameter")?;
                if !(2..=MAX_HOLES).contains(&count) {
                    return refuse(format!("A bolt circle has from 2 to {MAX_HOLES} holes."));
                }
                if pitch_diameter * (PI / f64::from(count)).sin() <= hole_diameter {
                    return refuse(
                        "The holes would run into each other: fewer, smaller, or a larger pitch circle.",
                    );
                }
                bolts(
                    &mut shapes,
                    center,
                    pitch_diameter,
                    count,
                    hole_diameter,
                    start_angle,
                );
            }
            Spec::RoundFlange {
                center,
                outer_diameter,
                bore_diameter,
                pitch_diameter,
                count,
                hole_diameter,
            } => {
                positive(outer_diameter, "outer diameter")?;
                positive(bore_diameter, "bore diameter")?;
                let bolt_circle = Spec::BoltCircle {
                    center,
                    pitch_diameter,
                    count,
                    hole_diameter,
                    start_angle: 0.0,
                };
                bolt_circle.shapes()?;
                if pitch_diameter + hole_diameter >= outer_diameter {
                    return refuse(
                        "The holes would cut the outside: a larger flange or a smaller pitch circle.",
                    );
                }
                if bore_diameter + hole_diameter >= pitch_diameter {
                    return refuse(
                        "The holes would cut the bore: a smaller bore or a larger pitch circle.",
                    );
                }
                let pitch = bolts(
                    &mut shapes,
                    center,
                    pitch_diameter,
                    count,
                    hole_diameter,
                    0.0,
                );
                let r = outer_diameter / 2.0;
                let outer = shapes.circle(center, r);
                let bore = shapes.circle(center, bore_diameter / 2.0);
                for circle in [outer, bore] {
                    shapes.geometric(
                        Kind::Concentric,
                        vec![(circle, Part::Edge), (pitch, Part::Edge)],
                    );
                }
                shapes.dimension(
                    "dia",
                    Kind::Diameter {
                        value: outer_diameter,
                    },
                    vec![(outer, Part::Edge)],
                    center + Vec2::new(r * 0.8, r * 1.1),
                );
                shapes.dimension(
                    "dia",
                    Kind::Diameter {
                        value: bore_diameter,
                    },
                    vec![(bore, Part::Edge)],
                    center + Vec2::new(-bore_diameter, bore_diameter * 0.3),
                );
            }
            Spec::RectangularFlange {
                center,
                width,
                height,
                bore_diameter,
                hole_diameter,
                offset,
            } => {
                positive(bore_diameter, "bore diameter")?;
                let corner = center - Vec2::new(width / 2.0, height / 2.0);
                let holes = CornerHoles {
                    diameter: hole_diameter,
                    offset,
                };
                let plate = Spec::Plate {
                    corner,
                    width,
                    height,
                    corners: Corners::Square,
                    holes: Some(holes),
                };
                plate.shapes()?;
                // The bore clear of the holes and the edges.
                let reach = (width / 2.0 - offset).hypot(height / 2.0 - offset);
                if bore_diameter / 2.0 + hole_diameter / 2.0 >= reach
                    || bore_diameter >= width.min(height)
                {
                    return refuse("The bore would cut the holes or the edges: a smaller bore.");
                }
                let sides = outline(&mut shapes, corner, width, height, Corners::Square);
                corner_holes(&mut shapes, &sides, corner, width, height, holes);
                let bore = shapes.circle(center, bore_diameter / 2.0);
                shapes.across(
                    (sides.left, Part::Start),
                    (bore, Part::Center),
                    width / 2.0,
                    center + Vec2::new(-width / 4.0, -bore_diameter),
                );
                shapes.up(
                    (sides.bottom, Part::Start),
                    (bore, Part::Center),
                    height / 2.0,
                    center + Vec2::new(-bore_diameter, -height / 4.0),
                );
                shapes.dimension(
                    "dia",
                    Kind::Diameter {
                        value: bore_diameter,
                    },
                    vec![(bore, Part::Edge)],
                    center + Vec2::new(bore_diameter * 0.7, bore_diameter * 0.7),
                );
            }
        }
        Ok(shapes)
    }

    /// Makes the part in `transaction`: its entities, then its constraints,
    /// named with the first numbers free. The entities' IDs, in order.
    pub fn place(
        &self,
        transaction: &mut TransactionBuilder<'_>,
    ) -> Result<Vec<EntityId>, PlaceError> {
        let shapes = self.shapes().map_err(PlaceError::Spec)?;
        let mut ids = Vec::with_capacity(shapes.entities.len());
        for entity in shapes.entities {
            ids.push(transaction.insert(entity).map_err(PlaceError::Drawing)?);
        }
        for template in shapes.templates {
            let references = template
                .references
                .iter()
                .map(|&(index, part)| Reference::new(ids[index], part))
                .collect();
            let mut constraint = match template.prefix {
                Some(prefix) => Constraint::dimensional(
                    transaction.free_constraint_name(prefix),
                    template.kind,
                    references,
                ),
                None => Constraint::geometric(template.kind, references),
            };
            constraint.place = template.place;
            transaction
                .add_constraint(constraint)
                .map_err(PlaceError::Drawing)?;
        }
        Ok(ids)
    }
}

/// Why a part was not placed.
#[derive(Clone, Debug, PartialEq)]
pub enum PlaceError {
    Spec(GenerateError),
    Drawing(TransactionError),
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spec(error) => error.fmt(f),
            Self::Drawing(error) => error.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Drawing;

    fn placed(spec: &Spec) -> Drawing {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        spec.place(&mut tx).unwrap();
        assert_eq!(tx.commit().refused, None);
        assert!(drawing.unsatisfied_constraints().is_empty());
        drawing
    }

    fn plate(corners: Corners, holes: Option<CornerHoles>) -> Spec {
        Spec::Plate {
            corner: Point2::ORIGIN,
            width: 120.0,
            height: 80.0,
            corners,
            holes,
        }
    }

    const HOLES: Option<CornerHoles> = Some(CornerHoles {
        diameter: 8.0,
        offset: 10.0,
    });

    #[test]
    fn every_part_is_placed_with_its_constraints_holding() {
        for spec in [
            plate(Corners::Square, None),
            plate(Corners::Square, HOLES),
            plate(Corners::Fillet(5.0), HOLES),
            plate(Corners::Chamfer(5.0), HOLES),
            Spec::Slot {
                center: Point2::new(10.0, 20.0),
                length: 40.0,
                width: 10.0,
                angle: 0.3,
            },
            Spec::HolePattern {
                first: Point2::ORIGIN,
                diameter: 6.0,
                rows: 2,
                columns: 3,
                row_spacing: 20.0,
                column_spacing: 15.0,
            },
            Spec::BoltCircle {
                center: Point2::ORIGIN,
                pitch_diameter: 100.0,
                count: 6,
                hole_diameter: 10.0,
                start_angle: 0.0,
            },
            Spec::RoundFlange {
                center: Point2::ORIGIN,
                outer_diameter: 150.0,
                bore_diameter: 50.0,
                pitch_diameter: 110.0,
                count: 8,
                hole_diameter: 12.0,
            },
            Spec::RectangularFlange {
                center: Point2::ORIGIN,
                width: 120.0,
                height: 120.0,
                bore_diameter: 50.0,
                hole_diameter: 12.0,
                offset: 15.0,
            },
        ] {
            let drawing = placed(&spec);
            assert!(!drawing.constraints().is_empty(), "{spec:?}");
        }
    }

    #[test]
    fn a_plate_with_holes_is_lines_and_circles_dimensioned_and_held() {
        let drawing = placed(&plate(Corners::Square, HOLES));
        let kinds: Vec<&str> = drawing.entities().map(|(_, e)| e.kind_name()).collect();
        assert_eq!(
            kinds,
            [
                "line", "line", "line", "line", "circle", "circle", "circle", "circle"
            ]
        );
        let names: Vec<String> = drawing
            .constraints()
            .iter()
            .filter(|(_, c)| c.kind.is_dimensional())
            .map(|(_, c)| c.name.clone())
            .collect();
        assert_eq!(
            names,
            [
                "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8", "d9", "d10", "dia1"
            ]
        );
    }

    /// The point of the constraints: a wider plate keeps its holes the
    /// same distance from its edges.
    #[test]
    fn a_plate_made_wider_keeps_its_holes_by_its_edges() {
        let mut drawing = placed(&plate(Corners::Fillet(5.0), HOLES));
        let d1 = drawing.constraints().find("d1").unwrap();
        let mut wider = drawing.constraints().get(d1).unwrap().clone();
        wider.kind = wider.kind.with_value(140.0);
        let mut tx = drawing.transaction();
        tx.replace_constraint(d1, wider).unwrap();
        assert_eq!(tx.commit().refused, None);
        assert!(drawing.unsatisfied_constraints().is_empty());
        let xs: Vec<f64> = drawing
            .entities()
            .filter_map(|(_, e)| match e {
                Entity::Circle(c) => Some(c.center.x),
                _ => None,
            })
            .collect();
        let right = drawing
            .entities()
            .filter_map(|(_, e)| e.as_line())
            .map(|l| l.start.x.max(l.end.x))
            .fold(f64::MIN, f64::max);
        let left = drawing
            .entities()
            .filter_map(|(_, e)| e.as_line())
            .map(|l| l.start.x.min(l.end.x))
            .fold(f64::MAX, f64::min);
        assert!((right - left - 140.0).abs() < 1e-6);
        assert!(
            (xs[0] - left - 10.0).abs() < 1e-6 && (right - xs[1] - 10.0).abs() < 1e-6,
            "{xs:?}"
        );
    }

    #[test]
    fn impossible_parts_are_refused_with_the_reason() {
        let refused = |spec: Spec| spec.shapes().unwrap_err().0;
        assert_eq!(
            refused(Spec::Plate {
                corner: Point2::ORIGIN,
                width: 0.0,
                height: 10.0,
                corners: Corners::Square,
                holes: None
            }),
            "The width must be more than zero."
        );
        assert_eq!(
            refused(plate(Corners::Fillet(45.0), None)),
            "The fillet radius is too large for the plate."
        );
        assert_eq!(
            refused(plate(
                Corners::Square,
                Some(CornerHoles {
                    diameter: 30.0,
                    offset: 10.0
                })
            )),
            "The holes would cut the edges: move them further in."
        );
        assert_eq!(
            refused(plate(
                Corners::Fillet(30.0),
                Some(CornerHoles {
                    diameter: 8.0,
                    offset: 10.0
                })
            )),
            "The holes would cut the corners: move them further in."
        );
        assert_eq!(
            refused(Spec::BoltCircle {
                center: Point2::ORIGIN,
                pitch_diameter: 20.0,
                count: 12,
                hole_diameter: 10.0,
                start_angle: 0.0
            }),
            "The holes would run into each other: fewer, smaller, or a larger pitch circle."
        );
        assert_eq!(
            refused(Spec::RoundFlange {
                center: Point2::ORIGIN,
                outer_diameter: 100.0,
                bore_diameter: 50.0,
                pitch_diameter: 110.0,
                count: 6,
                hole_diameter: 10.0
            }),
            "The holes would cut the outside: a larger flange or a smaller pitch circle."
        );
        assert_eq!(
            refused(Spec::HolePattern {
                first: Point2::ORIGIN,
                diameter: 10.0,
                rows: 2,
                columns: 1,
                row_spacing: 5.0,
                column_spacing: 0.0
            }),
            "The rows would run into each other: space them further apart."
        );
    }
}
