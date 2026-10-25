//! Proptest strategies for valid documents, for this crate's property
//! tests and for the engine's (`features = ["testing"]`).

use proptest::prelude::*;

use crate::validate::same_point;
use crate::{
    Color, Intent, Operation, Orientation, Pick, Point, SelectionWord, Targets, Unit, VERSION,
};

fn differ(a: Point, b: Point) -> bool {
    !same_point(a, b)
}

/// A finite coordinate in a workable range.
pub fn coordinate() -> impl Strategy<Value = f64> {
    (-1000i32..=1000, 0u8..100)
        .prop_map(|(whole, hundredths)| f64::from(whole) + f64::from(hundredths) / 100.0)
}

pub fn point() -> impl Strategy<Value = Point> {
    (coordinate(), coordinate()).prop_map(|(x, y)| [x, y])
}

/// Points with no two consecutive the same.
pub fn points(least: usize) -> impl Strategy<Value = Vec<Point>> {
    proptest::collection::vec(point(), least..least + 6)
        .prop_filter("consecutive points differ", |points| {
            points.windows(2).all(|pair| differ(pair[0], pair[1]))
        })
}

pub fn positive() -> impl Strategy<Value = f64> {
    (1u32..=5000).prop_map(|n| f64::from(n) / 10.0)
}

pub fn angle() -> impl Strategy<Value = f64> {
    (-360i32..=360).prop_map(f64::from)
}

/// An object ID the context might have given out.
pub fn id() -> impl Strategy<Value = u64> {
    1u64..=40
}

pub fn targets() -> impl Strategy<Value = Targets> {
    prop_oneof![
        Just(Targets::Selection(SelectionWord::Selection)),
        proptest::collection::vec(id(), 1..4).prop_map(Targets::Ids),
    ]
}

pub fn pick() -> impl Strategy<Value = Pick> {
    (id(), proptest::option::of(point())).prop_map(|(id, near)| Pick { id, near })
}

pub fn name() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9 _-]{0,15}".prop_filter("not blank", |s| !s.trim().is_empty())
}

pub fn text() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 .,!?-]{1,40}".prop_filter("not blank", |s| !s.trim().is_empty())
}

/// Two points that differ in both coordinates: a rectangle's corners.
fn corners() -> impl Strategy<Value = (Point, Point)> {
    (point(), point()).prop_filter("a rectangle has width and height", |(a, b)| {
        (a[0] - b[0]).abs() > 0.5 && (a[1] - b[1]).abs() > 0.5
    })
}

fn three_distinct() -> impl Strategy<Value = (Point, Point, Point)> {
    (point(), point(), point()).prop_filter("three distinct points", |(a, b, c)| {
        differ(*a, *b) && differ(*b, *c) && differ(*a, *c)
    })
}

/// One valid operation of any kind.
pub fn operation() -> impl Strategy<Value = Operation> {
    prop_oneof![
        points(2).prop_map(|points| Operation::Line { points }),
        (points(3), any::<bool>())
            .prop_map(|(points, closed)| Operation::Polyline { points, closed }),
        corners().prop_map(|(corner, opposite)| Operation::Rectangle { corner, opposite }),
        (point(), positive()).prop_map(|(center, radius)| Operation::Circle { center, radius }),
        three_distinct().prop_map(|(start, through, end)| Operation::Arc {
            start,
            through,
            end
        }),
        (3u32..=12, point(), positive(), any::<bool>()).prop_map(
            |(sides, center, radius, circumscribed)| Operation::Polygon {
                sides,
                center,
                radius,
                circumscribed,
            }
        ),
        (
            point(),
            text(),
            proptest::option::of(positive()),
            proptest::option::of(angle())
        )
            .prop_map(|(at, text, height, rotation)| Operation::Text {
                at,
                text,
                height,
                rotation,
            }),
        (targets(), point()).prop_map(|(targets, displacement)| Operation::Move {
            targets,
            displacement
        }),
        (targets(), point()).prop_map(|(targets, displacement)| Operation::Copy {
            targets,
            displacement
        }),
        (targets(), point(), angle()).prop_map(|(targets, base, angle)| Operation::Rotate {
            targets,
            base,
            angle
        }),
        (targets(), point(), positive()).prop_map(|(targets, base, factor)| Operation::Scale {
            targets,
            base,
            factor
        }),
        (targets(), point(), point(), any::<bool>())
            .prop_filter("an axis has two points", |(_, a, b, _)| differ(*a, *b))
            .prop_map(|(targets, a, b, keep)| Operation::Mirror {
                targets,
                axis: [a, b],
                keep,
            }),
        (pick(), positive(), point()).prop_map(|(target, distance, toward)| Operation::Offset {
            target,
            distance,
            toward
        }),
        (
            proptest::option::of(targets()),
            proptest::collection::vec(pick(), 1..3)
        )
            .prop_map(|(edges, targets)| Operation::Trim { edges, targets }),
        (
            proptest::option::of(targets()),
            proptest::collection::vec(pick(), 1..3)
        )
            .prop_map(|(edges, targets)| Operation::Extend { edges, targets }),
        (positive(), pick(), pick()).prop_map(|(radius, first, second)| Operation::Fillet {
            radius,
            first,
            second
        }),
        (positive(), positive(), pick(), pick()).prop_map(
            |(first_distance, second_distance, first, second)| Operation::Chamfer {
                first_distance,
                second_distance,
                first,
                second,
            }
        ),
        (targets(), 1u32..=4, 1u32..=4, coordinate(), coordinate())
            .prop_filter("an array makes copies", |(_, rows, columns, _, _)| rows
                * columns
                > 1)
            .prop_map(|(targets, rows, columns, row_spacing, column_spacing)| {
                Operation::Array {
                    targets,
                    rows,
                    columns,
                    row_spacing,
                    column_spacing,
                }
            }),
        (targets(), point(), 2u32..=12, angle(), any::<bool>())
            .prop_filter("a turn", |(_, _, _, angle, _)| angle.abs() > 0.5)
            .prop_map(
                |(targets, center, count, angle, rotate)| Operation::PolarArray {
                    targets,
                    center,
                    count,
                    angle,
                    rotate,
                }
            ),
        (name(), any::<bool>()).prop_map(|(name, current)| Operation::Layer { name, current }),
        (
            targets(),
            proptest::option::of(name()),
            proptest::option::of(prop_oneof![
                (1u16..=255).prop_map(Color::Index),
                Just(Color::Name("red".into()))
            ]),
            proptest::option::of(Just("Continuous".to_string())),
            proptest::option::of(Just("0.5".to_string())),
        )
            .prop_filter("something to set", |(_, l, c, t, w)| {
                l.is_some() || c.is_some() || t.is_some() || w.is_some()
            })
            .prop_map(|(targets, layer, color, linetype, lineweight)| {
                Operation::SetProperties {
                    targets,
                    layer,
                    color,
                    linetype,
                    lineweight,
                }
            }),
        (
            point(),
            point(),
            point(),
            proptest::option::of(prop_oneof![
                Just(Orientation::Horizontal),
                Just(Orientation::Vertical)
            ])
        )
            .prop_filter("two points to measure", |(a, b, _, _)| differ(*a, *b))
            .prop_map(|(from, to, at, orientation)| Operation::DimLinear {
                from,
                to,
                at,
                orientation,
            }),
        (point(), point(), point())
            .prop_filter("two points to measure", |(a, b, _)| differ(*a, *b))
            .prop_map(|(from, to, at)| Operation::DimAligned { from, to, at }),
        (pick(), point()).prop_map(|(target, at)| Operation::DimRadius { target, at }),
        (pick(), point()).prop_map(|(target, at)| Operation::DimDiameter { target, at }),
        (three_distinct(), point()).prop_map(|((vertex, first, second), at)| {
            Operation::DimAngular {
                vertex,
                first,
                second,
                at,
            }
        }),
        part(),
        placement(),
    ]
}

/// A placement (#273); the picks are arbitrary, so many find nothing to
/// touch, and the engine says so.
fn placement() -> impl Strategy<Value = Operation> {
    prop_oneof![
        (pick(), pick(), positive()).prop_map(|(first, second, radius)| {
            Operation::TangentCircle {
                first,
                second,
                third: None,
                radius: Some(radius),
            }
        }),
        (pick(), pick(), pick()).prop_map(|(first, second, third)| Operation::TangentCircle {
            first,
            second,
            third: Some(third),
            radius: None,
        }),
        (pick(), pick(), positive()).prop_map(|(first, second, radius)| Operation::TangentArc {
            first,
            second,
            radius
        }),
        (pick(), positive()).prop_map(|(of, radius)| Operation::ConcentricCircle { of, radius }),
        (targets(), pick(), 2u32..=12).prop_map(|(targets, path, count)| Operation::PathArray {
            targets,
            path,
            count: Some(count),
            spacing: None,
        }),
        (targets(), pick(), positive()).prop_map(|(targets, path, spacing)| {
            Operation::PathArray {
                targets,
                path,
                count: None,
                spacing: Some(spacing),
            }
        }),
        (
            targets(),
            point(),
            positive(),
            proptest::option::of(positive())
        )
            .prop_map(|(targets, inside, column_spacing, row_spacing)| {
                Operation::GridArray {
                    targets,
                    inside,
                    column_spacing,
                    row_spacing,
                }
            }),
    ]
}

/// A part (#272), any of them; many are impossible as parts — holes too
/// big for the plate — and the engine refuses those, as it should.
fn part() -> impl Strategy<Value = Operation> {
    prop_oneof![
        (
            point(),
            positive(),
            positive(),
            proptest::option::of(positive()),
            proptest::option::of((positive(), positive()))
        )
            .prop_map(|(corner, width, height, fillet, holes)| Operation::Plate {
                corner,
                width,
                height,
                fillet,
                chamfer: None,
                hole_diameter: holes.map(|(d, _)| d),
                hole_offset: holes.map(|(_, o)| o),
            }),
        (point(), positive(), positive(), angle()).prop_map(|(center, length, width, angle)| {
            Operation::Slot {
                center,
                length,
                width,
                angle,
            }
        }),
        (
            point(),
            positive(),
            1u32..4,
            1u32..4,
            positive(),
            positive()
        )
            .prop_map(
                |(first, diameter, rows, columns, row_spacing, column_spacing)| {
                    Operation::HolePattern {
                        first,
                        diameter,
                        rows,
                        columns,
                        row_spacing,
                        column_spacing,
                    }
                }
            ),
        (point(), positive(), 2u32..12, positive(), angle()).prop_map(
            |(center, pitch_diameter, count, hole_diameter, start_angle)| Operation::BoltCircle {
                center,
                pitch_diameter,
                count,
                hole_diameter,
                start_angle,
            }
        ),
        (
            point(),
            positive(),
            positive(),
            positive(),
            2u32..12,
            positive()
        )
            .prop_map(
                |(center, outer_diameter, bore_diameter, pitch_diameter, count, hole_diameter)| {
                    Operation::RoundFlange {
                        center,
                        outer_diameter,
                        bore_diameter,
                        pitch_diameter,
                        count,
                        hole_diameter,
                    }
                }
            ),
        (
            point(),
            positive(),
            positive(),
            positive(),
            positive(),
            positive()
        )
            .prop_map(
                |(center, width, height, bore_diameter, hole_diameter, hole_offset)| {
                    Operation::RectangularFlange {
                        center,
                        width,
                        height,
                        bore_diameter,
                        hole_diameter,
                        hole_offset,
                    }
                }
            ),
    ]
}

/// A valid document of up to `most` operations.
pub fn intent(most: usize) -> impl Strategy<Value = Intent> {
    (
        proptest::option::of(prop_oneof![
            Just(Unit::Mm),
            Just(Unit::Cm),
            Just(Unit::M),
            Just(Unit::In),
            Just(Unit::Ft)
        ]),
        proptest::collection::vec(operation(), 0..=most),
    )
        .prop_map(|(units, operations)| Intent {
            version: VERSION,
            units,
            operations,
        })
}
